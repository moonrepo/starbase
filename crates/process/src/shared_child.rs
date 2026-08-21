use crate::output::Output;
use crate::signal::*;
use bytes::Bytes;
use std::io;
use std::process::ExitStatus;
use std::sync::{Arc, OnceLock};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout};
use tokio::sync::Mutex;

/// How a child process ended.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChildExit {
    /// The process ran to completion with the given exit status.
    Completed(ExitStatus),

    /// Signalled with `SIGINT`
    Interrupted,

    /// Signalled with `SIGKILL`
    Killed,

    /// Signalled with anything else, carrying that signal. On Windows,
    /// where there are no signals, this is the code of the [`SignalType`]
    /// we asked for.
    Terminated(i32),
}

impl ChildExit {
    /// Return the signal that terminated the child, or `None` if it ran
    /// to completion.
    pub fn signal(&self) -> Option<i32> {
        match self {
            Self::Completed(_) => None,
            Self::Interrupted => Some(SignalType::Interrupt.get_code()),
            Self::Killed => Some(SignalType::Kill.get_code()),
            Self::Terminated(signal) => Some(*signal),
        }
    }
}

/// A cheaply cloneable handle to a running child process. Every clone
/// shares the same underlying process, so signalling or waiting on one
/// clone is visible to all others.
#[derive(Clone)]
pub struct SharedChild {
    control: Arc<ProcessControl>,
    inner: Arc<Mutex<Child>>,
    signal: Arc<OnceLock<SignalType>>,
    pid: u32,
    #[cfg(windows)]
    handle: RawHandle,
}

impl SharedChild {
    /// Wrap a spawned child so it can be shared across tasks.
    #[cfg(unix)]
    pub fn new(child: Child) -> Self {
        Self {
            control: Arc::new(ProcessControl::Direct),
            pid: child.id().unwrap(),
            inner: Arc::new(Mutex::new(child)),
            signal: Arc::new(OnceLock::new()),
        }
    }

    /// Wrap a spawned child so it can be shared across tasks.
    #[cfg(windows)]
    pub fn new(child: Child) -> Self {
        Self {
            control: Arc::new(ProcessControl::Direct),
            pid: child.id().unwrap(),
            handle: RawHandle(child.raw_handle().unwrap()),
            inner: Arc::new(Mutex::new(child)),
            signal: Arc::new(OnceLock::new()),
        }
    }

    /// Spawn a child in an owned process group or Job Object.
    pub(crate) async fn spawn_managed(command: &mut tokio::process::Command) -> io::Result<Self> {
        spawn_managed(command).await
    }

    /// Return the child's process id.
    pub fn id(&self) -> u32 {
        self.pid
    }

    /// Take the child's stdin pipe, if it was piped and not already taken.
    ///
    /// Dropping the returned handle closes the pipe, which the child sees
    /// as end of input. That is usually what you want once all input has
    /// been written.
    pub async fn take_stdin(&self) -> Option<ChildStdin> {
        self.inner.lock().await.stdin.take()
    }

    /// Take the child's stdout pipe, if it was piped and not already taken.
    ///
    /// Keep the returned handle alive for as long as the child may write.
    /// Dropping it closes our end of the pipe, and the child is killed by
    /// `SIGPIPE` on its next write, truncating its output partway through.
    /// [`Self::wait_with_output`] returns empty bytes for a pipe taken here.
    pub async fn take_stdout(&self) -> Option<ChildStdout> {
        self.inner.lock().await.stdout.take()
    }

    /// Take the child's stderr pipe, if it was piped and not already taken.
    ///
    /// The same `SIGPIPE` caveat as [`Self::take_stdout`] applies.
    pub async fn take_stderr(&self) -> Option<ChildStderr> {
        self.inner.lock().await.stderr.take()
    }

    /// Force kill the child immediately (`SIGKILL` on Unix, terminate on
    /// Windows), and wait for it to exit.
    pub async fn kill(&self) -> io::Result<ChildExit> {
        self.kill_with_signal(SignalType::Kill).await
    }

    /// Send `signal` to the child and wait for it to exit. The signal is
    /// remembered, so the resulting [`ChildExit`] reflects it even if the
    /// child's own exit status doesn't carry it (e.g. on Windows).
    pub async fn kill_with_signal(&self, signal: SignalType) -> io::Result<ChildExit> {
        self.signal.get_or_init(|| signal);

        self.control.signal(self, signal)?;

        // Acquire the child _after_ the kill command, otherwise it waits for
        // the command to finish running before killing, because the lock is
        // currently owned by `wait` or `wait_with_output`!
        self.wait().await
    }

    /// Wait for the child to exit, mapping a terminating signal onto the
    /// matching [`ChildExit`] variant.
    ///
    /// This returns as soon as the child exits, and does not wait on any
    /// process it may have spawned. Unlike [`Self::wait_with_output`], no
    /// pipes are read, so a child writing to a full pipe will block forever.
    pub async fn wait(&self) -> io::Result<ChildExit> {
        let mut child = self.inner.lock().await;
        let status = child.wait().await?;

        Ok(convert_exit_status(status, self.signal.get().copied()))
    }

    /// Wait for the direct child and clean up the owned process group or Job Object.
    pub(crate) async fn wait_managed(&self) -> io::Result<ChildExit> {
        #[cfg(unix)]
        if matches!(&*self.control, ProcessControl::ProcessGroup(_)) {
            let pid = self.pid;
            tokio::task::spawn_blocking(move || wait_unix_noreap(pid))
                .await
                .map_err(|error| io::Error::other(format!("process waiter panicked: {error}")))??;
            self.control.cleanup_remaining()?;

            return self.wait().await;
        }

        let exit = self.wait().await?;
        self.control.cleanup_remaining()?;

        Ok(exit)
    }

    /// Wait for the child to exit and drain its piped output.
    ///
    /// This returns once the pipes reach end of file, which is not always
    /// when the child exits. Any process that inherited the pipes holds
    /// them open, so a shell wrapper that backgrounds work keeps us here
    /// until that work finishes, and its output is captured too. To bound
    /// the wait, make the process being signalled the one holding the
    /// pipes (`exec` in a shell wrapper), as signalling a shell does not
    /// reach the processes it spawned.
    ///
    /// Pipes that were not requested, or that [`Self::take_stdout`] and
    /// friends already took, come back as empty bytes.
    // This method re-implements the tokio `wait_with_output` method
    // but does not take ownership of self. This is required to be able
    // to call `kill`, otherwise the child does not exist.
    pub async fn wait_with_output(&self) -> io::Result<Output> {
        use tokio::{io::AsyncReadExt, try_join};

        async fn read_to_end<A: AsyncReadExt + Unpin>(data: &mut Option<A>) -> io::Result<Vec<u8>> {
            let mut vec = Vec::new();

            if let Some(data) = data.as_mut() {
                data.read_to_end(&mut vec).await?;
            }

            Ok(vec)
        }

        let mut child = self.inner.lock().await;
        let mut stdout_pipe = child.stdout.take();
        let mut stderr_pipe = child.stderr.take();

        let stdout_fut = read_to_end(&mut stdout_pipe);
        let stderr_fut = read_to_end(&mut stderr_pipe);

        let (status, stdout, stderr) = try_join!(child.wait(), stdout_fut, stderr_fut)?;

        drop(stdout_pipe);
        drop(stderr_pipe);

        Ok(Output {
            exit: convert_exit_status(status, self.signal.get().copied()),
            stdout: Bytes::from(stdout),
            stderr: Bytes::from(stderr),
        })
    }
}

enum ProcessControl {
    Direct,
    #[cfg(unix)]
    ProcessGroup(i32),
    #[cfg(windows)]
    Job(JobObject),
}

impl ProcessControl {
    fn signal(&self, child: &SharedChild, signal: SignalType) -> io::Result<()> {
        match self {
            Self::Direct => {
                #[cfg(unix)]
                return kill(child.pid, signal);

                #[cfg(windows)]
                return kill(child.pid, child.handle.clone(), signal);

                #[allow(unreachable_code)]
                Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "process signals are not supported on this platform",
                ))
            }
            #[cfg(unix)]
            Self::ProcessGroup(pgid) => {
                let result = unsafe { libc::kill(-pgid, signal.get_code()) };

                if result == 0 {
                    Ok(())
                } else {
                    ignore_finished_group(Err(io::Error::last_os_error()))
                }
            }
            #[cfg(windows)]
            Self::Job(job) => match signal {
                SignalType::Interrupt => kill(child.pid, child.handle.clone(), signal),
                _ => job.terminate(),
            },
        }
    }

    fn cleanup_remaining(&self) -> io::Result<()> {
        match self {
            Self::Direct => Ok(()),
            #[cfg(unix)]
            Self::ProcessGroup(pgid) => {
                let result = unsafe { libc::kill(-pgid, libc::SIGKILL) };

                if result == 0 {
                    Ok(())
                } else {
                    ignore_finished_group(Err(io::Error::last_os_error()))
                }
            }
            #[cfg(windows)]
            Self::Job(job) => job.terminate(),
        }
    }
}

#[cfg(unix)]
async fn spawn_managed(command: &mut tokio::process::Command) -> io::Result<SharedChild> {
    use std::os::unix::process::CommandExt;

    command.as_std_mut().process_group(0);
    let child = command.spawn()?;
    let pid = child.id().expect("spawned child has a process id");

    Ok(SharedChild {
        control: Arc::new(ProcessControl::ProcessGroup(
            i32::try_from(pid).map_err(io::Error::other)?,
        )),
        inner: Arc::new(Mutex::new(child)),
        signal: Arc::new(OnceLock::new()),
        pid,
    })
}

#[cfg(unix)]
fn wait_unix_noreap(pid: u32) -> io::Result<()> {
    loop {
        let mut siginfo = std::mem::MaybeUninit::zeroed();
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                pid as libc::id_t,
                siginfo.as_mut_ptr(),
                libc::WEXITED | libc::WNOWAIT,
            )
        };

        if result == 0 {
            return Ok(());
        }

        let error = io::Error::last_os_error();

        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

#[cfg(unix)]
fn ignore_finished_group(result: io::Result<()>) -> io::Result<()> {
    match result {
        Err(error) if error.raw_os_error() == Some(libc::ESRCH) => Ok(()),
        #[cfg(target_vendor = "apple")]
        Err(error) if error.raw_os_error() == Some(libc::EPERM) => Ok(()),
        result => result,
    }
}

#[cfg(windows)]
async fn spawn_managed(command: &mut tokio::process::Command) -> io::Result<SharedChild> {
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::System::Threading::CREATE_SUSPENDED;

    command.as_std_mut().creation_flags(CREATE_SUSPENDED);
    let mut child = command.spawn()?;
    let pid = child.id().expect("spawned child has a process id");
    let process_handle = match child.raw_handle() {
        Some(handle) => handle as windows_sys::Win32::Foundation::HANDLE,
        None => {
            let error = io::Error::other("spawned child has no process handle");
            let cleanup = kill_and_wait_raw(&mut child).await.err();
            return Err(with_cleanup_error(error, cleanup));
        }
    };
    let job = match JobObject::create() {
        Ok(job) => job,
        Err(error) => {
            let cleanup = kill_and_wait_raw(&mut child).await.err();
            return Err(with_cleanup_error(error, cleanup));
        }
    };

    if let Err(error) = job.assign(process_handle) {
        let cleanup = kill_and_wait_raw(&mut child).await.err();
        return Err(with_cleanup_error(error, cleanup));
    }

    if let Err(error) = resume_process_threads(process_handle) {
        let cleanup = match job.terminate() {
            Ok(()) => child.wait().await.map(|_| ()),
            Err(error) => Err(error),
        }
        .err();
        return Err(with_cleanup_error(error, cleanup));
    }

    Ok(SharedChild {
        control: Arc::new(ProcessControl::Job(job)),
        handle: RawHandle(process_handle.cast()),
        inner: Arc::new(Mutex::new(child)),
        signal: Arc::new(OnceLock::new()),
        pid,
    })
}

#[cfg(windows)]
async fn kill_and_wait_raw(child: &mut Child) -> io::Result<()> {
    child.kill().await?;
    child.wait().await?;
    Ok(())
}

#[cfg(windows)]
fn with_cleanup_error(primary: io::Error, cleanup: Option<io::Error>) -> io::Error {
    match cleanup {
        Some(cleanup) => io::Error::new(
            primary.kind(),
            format!("{primary}; process cleanup also failed: {cleanup}"),
        ),
        None => primary,
    }
}

#[cfg(windows)]
struct JobObject(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
unsafe impl Send for JobObject {}

#[cfg(windows)]
unsafe impl Sync for JobObject {}

#[cfg(windows)]
impl JobObject {
    fn create() -> io::Result<Self> {
        use windows_sys::Win32::System::JobObjects::{
            CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        };

        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };

        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }

        let job = Self(handle);
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&limits).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };

        if configured == 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(job)
    }

    fn assign(&self, process: windows_sys::Win32::Foundation::HANDLE) -> io::Result<()> {
        let assigned = unsafe {
            windows_sys::Win32::System::JobObjects::AssignProcessToJobObject(self.0, process)
        };

        if assigned == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn terminate(&self) -> io::Result<()> {
        let terminated =
            unsafe { windows_sys::Win32::System::JobObjects::TerminateJobObject(self.0, 1) };

        if terminated == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

#[cfg(windows)]
impl Drop for JobObject {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(windows)]
fn resume_process_threads(process: windows_sys::Win32::Foundation::HANDLE) -> io::Result<()> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First,
                Thread32Next,
            },
            Threading::{GetProcessId, OpenThread, ResumeThread, THREAD_SUSPEND_RESUME},
        },
    };

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
            unsafe {
                CloseHandle(thread);
            }

            if resumed == u32::MAX {
                result = Err(io::Error::last_os_error());
                break;
            }
        }

        has_entry = unsafe { Thread32Next(snapshot, &mut entry) } != 0;
    }

    unsafe {
        CloseHandle(snapshot);
    }

    if result.is_ok() && !found {
        return Err(io::Error::other(
            "failed to find the suspended process thread",
        ));
    }

    result
}

fn convert_exit_status(status: ExitStatus, raw_signal: Option<SignalType>) -> ChildExit {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;

        if let Some(signal) = status.signal() {
            return match signal {
                libc::SIGINT => ChildExit::Interrupted,
                libc::SIGKILL => ChildExit::Killed,
                other => ChildExit::Terminated(other),
            };
        }
    }

    // The Unix signal above sometimes doesn't capture the correct
    // wait status, so to support those edges, and Windows in general,
    // we'll read the raw signal that we explicitly used
    if let Some(signal) = raw_signal {
        return match signal {
            SignalType::Interrupt => ChildExit::Interrupted,
            SignalType::Kill => ChildExit::Killed,
            other => ChildExit::Terminated(other.get_code()),
        };
    }

    ChildExit::Completed(status)
}
