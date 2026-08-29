use crate::process_registry::{ManagedRegistration, ManagedRequest, ProcessRegistry};
use crate::{ChildExit, Command, Output, ProcessError, SharedChild, SignalType};
use bytes::{Bytes, BytesMut};
use starbase_console::Reporter;
use std::future::pending;
use std::io;
use std::process::{Command as StdCommand, Stdio};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{ChildStderr, ChildStdout, Command as TokioCommand};
use tokio::runtime::{Handle, RuntimeFlavor};
use tokio::time::sleep;
use tracing::debug;

const READ_BUFFER_SIZE: usize = 64 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(2);
const FAILURE_DRAIN_TIMEOUT: Duration = Duration::from_secs(1);

/// Bounds for synchronous process capture.
///
/// By default, process runtime and combined output are unbounded, while
/// output draining after the direct child exits is limited to one second.
#[derive(Clone, Debug)]
pub struct CaptureOptions {
    /// Maximum combined bytes captured from stdout and stderr. Defaults to
    /// `None`, which does not limit captured output.
    pub output_limit: Option<usize>,

    /// Maximum duration the process may run before it is terminated. Defaults
    /// to `None`, which does not limit process runtime.
    pub timeout: Option<Duration>,

    /// Maximum duration to drain stdout and stderr after the process exits.
    /// Defaults to one second. Set to `None` to wait until both pipes close.
    pub output_drain_timeout: Option<Duration>,
}

impl Default for CaptureOptions {
    fn default() -> Self {
        Self {
            output_limit: None,
            timeout: None,
            output_drain_timeout: Some(Duration::from_secs(1)),
        }
    }
}

impl<R: Reporter> Command<R> {
    /// Execute the command synchronously and capture stdout and stderr in memory.
    ///
    /// This mode does not support buffered input, output caching, or continuous
    /// pipes. Stdin is closed when the child starts. The process is registered
    /// for request-only shutdown if a [`ProcessRegistry`] already exists.
    pub fn exec_capture_output_to_memory_blocking(
        &mut self,
        options: &CaptureOptions,
    ) -> miette::Result<Output> {
        self.validate_blocking_capture()?;

        let bin = self.get_bin_name();
        let registry = ProcessRegistry::try_instance();
        let runtime = Handle::try_current().ok();

        validate_runtime_context(&bin, registry.as_deref(), runtime.as_ref())?;

        let mut command = self.create_sync_command()?;
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let options = options.clone();
        let worker_bin = bin.clone();
        let instant = Instant::now();
        let run = || {
            let (pid_sender, pid_receiver) = mpsc::sync_channel(1);
            let worker = thread::Builder::new()
                .name("starbase-process-capture".into())
                .spawn(move || {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|error| ProcessError::Capture {
                            bin: worker_bin.clone(),
                            error: Box::new(error),
                        })?;

                    runtime.block_on(capture_process(
                        command,
                        &options,
                        &worker_bin,
                        registry,
                        pid_sender,
                    ))
                })
                .map_err(|error| ProcessError::Capture {
                    bin: bin.clone(),
                    error: Box::new(error),
                })?;

            if let Ok(pid) = pid_receiver.recv() {
                self.pre_log_command_pid(pid);
            }

            worker.join().map_err(|_| ProcessError::Capture {
                bin: bin.clone(),
                error: Box::new(io::Error::other("process capture worker panicked")),
            })?
        };

        let output = if runtime
            .as_ref()
            .is_some_and(|handle| handle.runtime_flavor() == RuntimeFlavor::MultiThread)
        {
            tokio::task::block_in_place(run)
        } else {
            run()
        }?;

        debug!("Ran command in {:?}", instant.elapsed());
        self.handle_nonzero_status(&output, true)?;

        Ok(output)
    }

    fn validate_blocking_capture(&self) -> Result<(), ProcessError> {
        let option = if self.should_pass_stdin() {
            Some("buffered input")
        } else if self.should_cache_output() {
            Some("output caching")
        } else if self.continuous_pipe {
            Some("continuous pipes")
        } else {
            None
        };

        match option {
            Some(option) => Err(ProcessError::UnsupportedCaptureOption {
                bin: self.get_bin_name(),
                option,
            }),
            None => Ok(()),
        }
    }
}

fn validate_runtime_context(
    bin: &str,
    registry: Option<&ProcessRegistry>,
    runtime: Option<&Handle>,
) -> Result<(), ProcessError> {
    if registry.is_some()
        && runtime.is_some_and(|handle| handle.runtime_flavor() == RuntimeFlavor::CurrentThread)
    {
        Err(ProcessError::UnsupportedCaptureRuntime {
            bin: bin.to_owned(),
        })
    } else {
        Ok(())
    }
}

async fn capture_process(
    command: StdCommand,
    options: &CaptureOptions,
    bin: &str,
    registry: Option<Arc<ProcessRegistry>>,
    pid_sender: mpsc::SyncSender<u32>,
) -> Result<Output, ProcessError> {
    let registration = registry
        .as_ref()
        .map(|registry| registry.register_managed());

    if let Some(request) = registration.as_ref().map(ManagedRegistration::current)
        && request != ManagedRequest::Running
    {
        return Err(interruption(bin, request));
    }

    let mut command = TokioCommand::from(command);
    let managed = platform::spawn(&mut command)
        .await
        .map_err(|error| ProcessError::Capture {
            bin: bin.to_owned(),
            error: Box::new(error),
        })?;
    let _ = pid_sender.send(managed.child.id());

    let stdout = managed.child.take_stdout().await;
    let stderr = managed.child.take_stderr().await;
    let missing_pipe = if stdout.is_none() {
        Some("failed to capture stdout")
    } else if stderr.is_none() {
        Some("failed to capture stderr")
    } else {
        None
    };
    let mut capture = CaptureCoordinator::new(
        managed,
        CapturedPipes::new(stdout, stderr, options.output_limit),
        registration,
        options,
        bin,
    );

    if let Some(message) = missing_pipe {
        let failure = capture.capture_error(io::Error::other(message));
        Err(capture.cleanup(failure).await)
    } else {
        capture.run().await
    }
}

struct CaptureCoordinator<'a> {
    managed: ManagedChild,
    pipes: CapturedPipes,
    registration: Option<ManagedRegistration>,
    options: &'a CaptureOptions,
    bin: &'a str,
}

enum CaptureEvent {
    Output(io::Result<bool>),
    Request(ManagedRequest),
    Poll,
}

impl<'a> CaptureCoordinator<'a> {
    fn new(
        managed: ManagedChild,
        pipes: CapturedPipes,
        registration: Option<ManagedRegistration>,
        options: &'a CaptureOptions,
        bin: &'a str,
    ) -> Self {
        Self {
            managed,
            pipes,
            registration,
            options,
            bin,
        }
    }

    async fn run(mut self) -> Result<Output, ProcessError> {
        let started = Instant::now();

        loop {
            match self.managed.child.managed_has_exited() {
                Ok(true) => return self.finish_normal().await,
                Ok(false) => {}
                Err(error) => {
                    let failure = self.capture_error(error);
                    return Err(self.cleanup(failure).await);
                }
            }

            if self
                .options
                .timeout
                .is_some_and(|timeout| started.elapsed() >= timeout)
            {
                let failure = ProcessError::Timeout {
                    bin: self.bin.to_owned(),
                    timeout: self.options.timeout.expect("deadline requires a timeout"),
                    output: None,
                };
                return Err(self.cleanup(failure).await);
            }

            match self.next_event().await {
                CaptureEvent::Output(result) => {
                    if let Err(failure) = self.output_result(result) {
                        return Err(self.cleanup(failure).await);
                    }
                }
                CaptureEvent::Request(request) => return Err(self.cancel(request).await),
                CaptureEvent::Poll => {}
            }
        }
    }

    async fn finish_normal(&mut self) -> Result<Output, ProcessError> {
        let started = Instant::now();

        let drain_failure = loop {
            if self.pipes.finished() {
                break None;
            }

            if self
                .options
                .output_drain_timeout
                .is_some_and(|timeout| started.elapsed() >= timeout)
            {
                break Some(ProcessError::OutputDrainTimeout {
                    bin: self.bin.to_owned(),
                    timeout: self
                        .options
                        .output_drain_timeout
                        .expect("deadline requires a drain timeout"),
                    output: None,
                });
            }

            match self.next_event().await {
                CaptureEvent::Output(result) => {
                    if let Err(failure) = self.output_result(result) {
                        return Err(self.cleanup(failure).await);
                    }
                }
                CaptureEvent::Request(request) => return Err(self.cancel(request).await),
                CaptureEvent::Poll => {}
            }
        };

        if let Some(request) = self.close_registration() {
            return Err(self.cancel(request).await);
        }

        if let Err(error) = self.managed.control.normal_completion() {
            return Err(match drain_failure {
                Some(failure) => {
                    let cleanup = self.cleanup_after_exit(failure).await;
                    self.cleanup_error(error, cleanup, Default::default())
                }
                None => {
                    let failure = self.finalization_error(error, Default::default());
                    self.cleanup_after_exit(failure).await
                }
            });
        }

        match (drain_failure, self.managed.child.managed_wait(None).await) {
            (None, Ok(exit)) => Ok(self.pipes.take_output(exit)),
            (None, Err(error)) => {
                let partial_bytes = self.pipes.take_bytes();
                Err(self.finalization_error(error, partial_bytes))
            }
            (Some(mut failure), Ok(exit)) => {
                failure.attach_partial_output(self.pipes.take_output(exit));
                Err(failure)
            }
            (Some(failure), Err(error)) => {
                let partial_bytes = self.pipes.take_bytes();
                Err(self.cleanup_error(error, failure, partial_bytes))
            }
        }
    }

    async fn cancel(&mut self, request: ManagedRequest) -> ProcessError {
        let signal = request.signal();
        let failure = interruption(self.bin, request);

        if !matches!(request, ManagedRequest::Graceful(_)) {
            return self.cleanup(failure).await;
        }

        if let Err(error) = self.managed.control.signal(signal) {
            return self.cleanup_with(error, failure).await;
        }

        loop {
            match self.managed.child.managed_has_exited() {
                Ok(true) => return self.cleanup_after_signal(failure, signal).await,
                Ok(false) => {}
                Err(error) => return self.cleanup_with(error, failure).await,
            }

            match self.next_event().await {
                CaptureEvent::Output(result) => {
                    if result.is_err() || result.is_ok_and(|exceeded| exceeded) {
                        return self.cleanup(failure).await;
                    }
                }
                CaptureEvent::Request(ManagedRequest::Force(_)) => {
                    return self.cleanup(failure).await;
                }
                CaptureEvent::Request(_) | CaptureEvent::Poll => {}
            }
        }
    }

    async fn cleanup(&mut self, failure: ProcessError) -> ProcessError {
        self.cleanup_managed(failure, false, Some(SignalType::Kill))
            .await
    }

    async fn cleanup_after_exit(&mut self, failure: ProcessError) -> ProcessError {
        self.cleanup_managed(failure, true, None).await
    }

    async fn cleanup_after_signal(
        &mut self,
        failure: ProcessError,
        signal: SignalType,
    ) -> ProcessError {
        self.cleanup_managed(failure, true, Some(signal)).await
    }

    async fn cleanup_managed(
        &mut self,
        mut failure: ProcessError,
        child_exited: bool,
        exit_signal: Option<SignalType>,
    ) -> ProcessError {
        let mut signal_error = self.managed.control.signal(SignalType::Kill).err();
        let direct_kill_error = if child_exited {
            None
        } else {
            match self.managed.child.managed_start_kill().await {
                Ok(()) => None,
                Err(error) => match self.managed.child.managed_has_exited() {
                    Ok(true) => None,
                    Ok(false) => Some(error),
                    Err(wait_error) => Some(combine_errors(error, wait_error)),
                },
            }
        };

        if let Some(error) = direct_kill_error {
            signal_error = Some(match signal_error {
                Some(signal_error) => combine_errors(signal_error, error),
                None => error,
            });

            if !self.managed.child.managed_has_exited().unwrap_or(false) {
                self.drain_failure().await;
                let partial_bytes = self.pipes.take_bytes();
                let error = signal_error
                    .take()
                    .expect("a failed direct kill records a cleanup error");

                return self.cleanup_error(error, failure, partial_bytes);
            }
        }

        let wait = self.managed.child.managed_wait(exit_signal).await;
        self.drain_failure().await;

        match wait {
            Ok(exit) => {
                let output = self.pipes.take_output(exit);
                failure.attach_partial_output(output);

                match signal_error {
                    Some(error) => self.cleanup_error(error, failure, Default::default()),
                    None => failure,
                }
            }
            Err(error) => {
                let error = match signal_error {
                    Some(signal_error) => combine_errors(signal_error, error),
                    None => error,
                };
                let partial_bytes = self.pipes.take_bytes();

                self.cleanup_error(error, failure, partial_bytes)
            }
        }
    }

    async fn cleanup_with(&mut self, error: io::Error, failure: ProcessError) -> ProcessError {
        let cleanup = self.cleanup(failure).await;
        self.cleanup_error(error, cleanup, Default::default())
    }

    async fn next_event(&mut self) -> CaptureEvent {
        tokio::select! {
            result = self.pipes.read_once() => CaptureEvent::Output(result),
            request = next_request(&mut self.registration) => CaptureEvent::Request(request),
            _ = sleep(POLL_INTERVAL) => CaptureEvent::Poll,
        }
    }

    fn output_result(&self, result: io::Result<bool>) -> Result<(), ProcessError> {
        match result {
            Ok(false) => Ok(()),
            Ok(true) => Err(ProcessError::OutputLimitExceeded {
                bin: self.bin.to_owned(),
                limit: self
                    .options
                    .output_limit
                    .expect("a limit generated the limit error"),
                output: None,
            }),
            Err(error) => Err(self.capture_error(error)),
        }
    }

    fn close_registration(&mut self) -> Option<ManagedRequest> {
        self.registration
            .as_mut()
            .and_then(ManagedRegistration::close)
    }

    async fn drain_failure(&mut self) {
        self.pipes
            .drain_bounded(
                self.options
                    .output_drain_timeout
                    .unwrap_or(FAILURE_DRAIN_TIMEOUT),
            )
            .await;
    }

    fn capture_error(&self, error: io::Error) -> ProcessError {
        ProcessError::Capture {
            bin: self.bin.to_owned(),
            error: Box::new(error),
        }
    }

    fn cleanup_error(
        &self,
        error: io::Error,
        failure: ProcessError,
        partial_bytes: (Bytes, Bytes),
    ) -> ProcessError {
        match failure {
            ProcessError::Cleanup {
                error: cleanup_error,
                failure,
                output,
                partial_bytes,
                ..
            } => ProcessError::Cleanup {
                bin: self.bin.to_owned(),
                error: Box::new(combine_errors(error, *cleanup_error)),
                failure,
                output,
                partial_bytes,
            },
            failure => ProcessError::Cleanup {
                bin: self.bin.to_owned(),
                error: Box::new(error),
                output: failure.partial_output().cloned(),
                failure: Box::new(failure),
                partial_bytes: Box::new(partial_bytes),
            },
        }
    }

    fn finalization_error(&self, error: io::Error, partial_bytes: (Bytes, Bytes)) -> ProcessError {
        ProcessError::Finalization {
            bin: self.bin.to_owned(),
            error: Box::new(error),
            output: None,
            partial_bytes: Box::new(partial_bytes),
        }
    }
}

async fn next_request(registration: &mut Option<ManagedRegistration>) -> ManagedRequest {
    match registration {
        Some(registration) => registration.changed().await,
        None => pending().await,
    }
}

struct CapturedPipes {
    stdout: PipeCapture<ChildStdout>,
    stderr: PipeCapture<ChildStderr>,
    remaining: usize,
}

impl CapturedPipes {
    fn new(stdout: Option<ChildStdout>, stderr: Option<ChildStderr>, limit: Option<usize>) -> Self {
        Self {
            stdout: PipeCapture::new(stdout),
            stderr: PipeCapture::new(stderr),
            remaining: limit.unwrap_or(usize::MAX),
        }
    }

    async fn read_once(&mut self) -> io::Result<bool> {
        let remaining = self.remaining;
        let (result, stdout) = tokio::select! {
            result = self.stdout.read_once(remaining) => (result, true),
            result = self.stderr.read_once(remaining) => (result, false),
        };
        let data = result?;
        let Some(data) = data else {
            return Ok(false);
        };
        let exceeded = data.len() > self.remaining;
        let keep = data.len().min(self.remaining);

        if stdout {
            self.stdout.output.extend_from_slice(&data[..keep]);
        } else {
            self.stderr.output.extend_from_slice(&data[..keep]);
        }
        self.remaining -= keep;

        Ok(exceeded)
    }

    fn finished(&self) -> bool {
        self.stdout.pipe.is_none() && self.stderr.pipe.is_none()
    }

    async fn drain_bounded(&mut self, timeout: Duration) {
        let started = Instant::now();

        while !self.finished() && started.elapsed() < timeout {
            tokio::select! {
                result = self.read_once() => {
                    if result.is_err() {
                        break;
                    }
                }
                _ = sleep(POLL_INTERVAL) => {}
            }
        }
    }

    fn take_bytes(&mut self) -> (Bytes, Bytes) {
        (
            std::mem::take(&mut self.stdout.output).freeze(),
            std::mem::take(&mut self.stderr.output).freeze(),
        )
    }

    fn take_output(&mut self, exit: ChildExit) -> Output {
        let (stdout, stderr) = self.take_bytes();
        Output {
            exit,
            stdout,
            stderr,
        }
    }
}

struct PipeCapture<T> {
    pipe: Option<T>,
    output: BytesMut,
}

impl<T: AsyncRead + Unpin> PipeCapture<T> {
    fn new(pipe: Option<T>) -> Self {
        Self {
            pipe,
            output: BytesMut::new(),
        }
    }

    async fn read_once(&mut self, remaining: usize) -> io::Result<Option<Vec<u8>>> {
        let Some(pipe) = self.pipe.as_mut() else {
            return pending().await;
        };

        let mut buffer = vec![0; remaining.saturating_add(1).min(READ_BUFFER_SIZE)];
        let count = pipe.read(&mut buffer).await?;

        if count == 0 {
            self.pipe.take();
            Ok(None)
        } else {
            buffer.truncate(count);
            Ok(Some(buffer))
        }
    }
}

struct ManagedChild {
    child: SharedChild,
    control: platform::ManagedGroup,
}

impl Drop for ManagedChild {
    fn drop(&mut self) {
        self.control.abandon();
    }
}

fn interruption(bin: &str, request: ManagedRequest) -> ProcessError {
    ProcessError::Interrupted {
        bin: bin.to_owned(),
        signal: request.signal(),
        output: None,
    }
}

fn combine_errors(first: io::Error, second: io::Error) -> io::Error {
    io::Error::new(
        first.kind(),
        format!("{first}; process cleanup also failed: {second}"),
    )
}

#[cfg(unix)]
mod platform {
    use super::*;
    use crate::signal::kill_process_group;
    use std::os::unix::process::CommandExt;

    pub struct ManagedGroup {
        pgid: u32,
        armed: bool,
    }

    impl ManagedGroup {
        pub fn signal(&mut self, signal: SignalType) -> io::Result<()> {
            let result = kill_process_group(self.pgid, signal);

            if signal == SignalType::Kill && result.is_ok() {
                self.armed = false;
            }

            result
        }

        pub fn normal_completion(&mut self) -> io::Result<()> {
            self.armed = false;
            Ok(())
        }

        pub fn abandon(&mut self) {
            if self.armed {
                let _ = kill_process_group(self.pgid, SignalType::Kill);
                self.armed = false;
            }
        }
    }

    pub async fn spawn(command: &mut TokioCommand) -> io::Result<ManagedChild> {
        command.as_std_mut().process_group(0);
        command.kill_on_drop(true);
        let child = command.spawn()?;
        let shared = SharedChild::new(child);
        let pgid = shared.id();

        Ok(ManagedChild {
            child: shared,
            control: ManagedGroup { pgid, armed: true },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_active_registry_from_current_thread_runtime() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        runtime.block_on(async {
            let registry = ProcessRegistry::new(10);
            let handle = Handle::current();
            let error =
                validate_runtime_context("test", Some(&registry), Some(&handle)).unwrap_err();

            assert!(matches!(
                error,
                ProcessError::UnsupportedCaptureRuntime { ref bin } if bin == "test"
            ));
            drop(registry);
        });
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancellation_during_output_drain_terminates_descendants() {
        use std::path::PathBuf;
        use std::time::{SystemTime, UNIX_EPOCH};

        fn temp_path() -> PathBuf {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();

            std::env::temp_dir().join(format!(
                "starbase-process-cancel-drain-{}-{nonce}",
                std::process::id()
            ))
        }

        let marker = temp_path();
        let script = format!(
            "(trap '' TERM; sleep 0.3; touch '{}') & exit 0",
            marker.display()
        );
        let mut command = StdCommand::new("bash");
        command
            .args(["-c", &script])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let registry = Arc::new(ProcessRegistry::new(50));
        let cancellation_registry = Arc::clone(&registry);
        let (pid_sender, pid_receiver) = mpsc::sync_channel(1);

        let capture = tokio::spawn(async move {
            capture_process(
                command,
                &CaptureOptions {
                    output_drain_timeout: Some(Duration::from_secs(1)),
                    ..CaptureOptions::default()
                },
                "bash",
                Some(registry),
                pid_sender,
            )
            .await
        });

        let pid = loop {
            if let Ok(pid) = pid_receiver.try_recv() {
                break pid;
            }

            sleep(Duration::from_millis(1)).await;
        };

        loop {
            let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
            let result = unsafe {
                libc::waitid(
                    libc::P_PID,
                    pid as libc::id_t,
                    info.as_mut_ptr(),
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            };
            assert_eq!(result, 0);

            if unsafe { info.assume_init().si_pid() } != 0 {
                break;
            }

            sleep(Duration::from_millis(1)).await;
        }

        cancellation_registry.terminate_running();

        let error = capture.await.unwrap().unwrap_err();
        assert!(matches!(error, ProcessError::Interrupted { .. }));

        sleep(Duration::from_millis(400)).await;
        assert!(!marker.exists());
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject,
    };
    use windows_sys::Win32::System::Threading::{
        CREATE_SUSPENDED, GetProcessId, OpenThread, ResumeThread, THREAD_SUSPEND_RESUME,
    };

    pub struct ManagedGroup {
        job: Option<JobObject>,
    }

    impl ManagedGroup {
        pub fn signal(&mut self, signal: SignalType) -> io::Result<()> {
            match signal {
                SignalType::Interrupt => Ok(()),
                _ => self
                    .job
                    .as_ref()
                    .expect("running child owns its Job Object")
                    .terminate(),
            }
        }

        pub fn normal_completion(&mut self) -> io::Result<()> {
            if let Some(job) = &self.job {
                job.set_kill_on_close(false)?;
            }
            self.job.take();
            Ok(())
        }

        pub fn abandon(&mut self) {}
    }

    pub async fn spawn(command: &mut TokioCommand) -> io::Result<ManagedChild> {
        command.as_std_mut().creation_flags(CREATE_SUSPENDED);
        command.kill_on_drop(true);
        let mut child = command.spawn()?;
        let process = match child.raw_handle() {
            Some(handle) => handle as HANDLE,
            None => {
                let error = io::Error::other("spawned child has no process handle");
                return Err(rollback_suspended(&mut child, error).await);
            }
        };
        let job = match JobObject::create().and_then(|job| {
            job.assign(process)?;
            Ok(job)
        }) {
            Ok(job) => job,
            Err(error) => return Err(rollback_suspended(&mut child, error).await),
        };

        if let Err(error) = resume_process_threads(process) {
            return Err(rollback_job(&mut child, &job, error).await);
        }

        Ok(ManagedChild {
            child: SharedChild::new(child),
            control: ManagedGroup { job: Some(job) },
        })
    }

    async fn rollback_suspended(
        child: &mut tokio::process::Child,
        primary: io::Error,
    ) -> io::Error {
        match child.kill().await {
            Ok(()) => primary,
            Err(cleanup) => combine_errors(primary, cleanup),
        }
    }

    async fn rollback_job(
        child: &mut tokio::process::Child,
        job: &JobObject,
        primary: io::Error,
    ) -> io::Error {
        let termination = job.terminate();
        let cleanup = match termination {
            Ok(()) => child.wait().await.map(|_| ()),
            Err(error) => Err(error),
        };

        match cleanup {
            Ok(()) => primary,
            Err(cleanup) => combine_errors(primary, cleanup),
        }
    }

    struct JobObject(HANDLE);

    impl JobObject {
        fn create() -> io::Result<Self> {
            let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            let job = Self(handle);
            job.set_kill_on_close(true)?;
            Ok(job)
        }

        fn assign(&self, process: HANDLE) -> io::Result<()> {
            if unsafe { AssignProcessToJobObject(self.0, process) } == 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }

        fn terminate(&self) -> io::Result<()> {
            if unsafe { TerminateJobObject(self.0, 1) } == 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }

        fn set_kill_on_close(&self, enabled: bool) -> io::Result<()> {
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            if enabled {
                limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            }
            let result = unsafe {
                SetInformationJobObject(
                    self.0,
                    JobObjectExtendedLimitInformation,
                    std::ptr::from_ref(&limits).cast(),
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
            };
            if result == 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
    }

    impl Drop for JobObject {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }

    fn resume_process_threads(process: HANDLE) -> io::Result<()> {
        let process_id = unsafe { GetProcessId(process) };
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }

        let mut entry = THREADENTRY32 {
            dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
            ..THREADENTRY32::default()
        };
        let mut found = false;
        let mut has_entry = unsafe { Thread32First(snapshot, &mut entry) } != 0;
        let mut result = Ok(());

        while has_entry {
            if entry.th32OwnerProcessID == process_id {
                let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
                if thread.is_null() {
                    result = Err(io::Error::last_os_error());
                    break;
                }
                found = true;
                let resumed = unsafe { ResumeThread(thread) };
                unsafe { CloseHandle(thread) };
                if resumed == u32::MAX {
                    result = Err(io::Error::last_os_error());
                    break;
                }
            }
            has_entry = unsafe { Thread32Next(snapshot, &mut entry) } != 0;
        }

        unsafe { CloseHandle(snapshot) };
        if result.is_ok() && !found {
            return Err(io::Error::other(
                "failed to find the suspended process thread",
            ));
        }
        result
    }
}
