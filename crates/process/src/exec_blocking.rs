use crate::{ChildExit, Command, Output, ProcessError, SharedChild, SignalType};
use bytes::{Bytes, BytesMut};
use starbase_console::Reporter;
use std::ffi::OsStr;
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Command as TokioCommand;
use tokio::task::JoinSet;
use tokio::time::{Instant as TokioInstant, timeout_at};
use tracing::debug;

const READ_BUFFER_SIZE: usize = 64 * 1024;

/// Bounds for synchronous process capture.
#[derive(Clone, Debug)]
pub struct CaptureOptions {
    /// Maximum combined bytes captured from stdout and stderr.
    pub output_limit: Option<usize>,

    /// Maximum duration the process may run before it is terminated.
    pub timeout: Option<Duration>,

    /// Maximum duration to drain stdout and stderr after the process exits.
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

enum WorkerResult {
    Child(io::Result<ChildExit>),
    Stdin(io::Result<()>),
    Stderr(Result<Bytes, ReadFailure>),
    Stdout(Result<Bytes, ReadFailure>),
}

struct ReadFailure {
    error: io::Error,
    output: Bytes,
}

impl<R: Reporter> Command<R> {
    /// Execute the command synchronously and capture stdout and stderr in memory.
    pub fn exec_capture_output_to_memory_blocking(
        &mut self,
        options: &CaptureOptions,
    ) -> miette::Result<Output> {
        self.exec_blocking(options)
    }

    fn exec_blocking(&mut self, options: &CaptureOptions) -> miette::Result<Output> {
        let instant = Instant::now();
        let bin = self.get_bin_name();
        let input = self
            .should_pass_stdin()
            .then(|| Bytes::copy_from_slice(self.input.join(OsStr::new(" ")).as_encoded_bytes()));
        let command = self.create_async_command()?;
        let options = options.clone();
        let (pid_tx, pid_rx) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("starbase-process-capture".into())
            .spawn(move || {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|error| ProcessError::Capture {
                        bin: bin.clone(),
                        error: Box::new(error),
                    })?
                    .block_on(capture_process(command, input, options, bin, pid_tx))
            })
            .map_err(|error| ProcessError::Capture {
                bin: self.get_bin_name(),
                error: Box::new(error),
            })?;

        let pid = pid_rx.recv().ok();

        if let Some(pid) = pid {
            self.pre_log_command_pid(pid);
        }

        let output = worker.join().map_err(|_| ProcessError::Capture {
            bin: self.get_bin_name(),
            error: Box::new(io::Error::other("process capture thread panicked")),
        })??;

        if let Some(pid) = pid {
            debug!(pid, "Ran command in {:?}", instant.elapsed());
        }

        self.handle_nonzero_status(&output, true)?;

        Ok(output)
    }
}

async fn capture_process(
    mut command: TokioCommand,
    input: Option<Bytes>,
    options: CaptureOptions,
    bin: String,
    pid_tx: mpsc::SyncSender<u32>,
) -> Result<Output, ProcessError> {
    use std::process::Stdio;

    command.stdin(if input.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let child = SharedChild::spawn_managed(&mut command)
        .await
        .map_err(|error| ProcessError::Capture {
            bin: bin.clone(),
            error: Box::new(error),
        })?;
    let _ = pid_tx.send(child.id());

    let stdout = child
        .take_stdout()
        .await
        .ok_or_else(|| ProcessError::Capture {
            bin: bin.clone(),
            error: Box::new(io::Error::other("failed to capture stdout")),
        })?;
    let stderr = child
        .take_stderr()
        .await
        .ok_or_else(|| ProcessError::Capture {
            bin: bin.clone(),
            error: Box::new(io::Error::other("failed to capture stderr")),
        })?;
    let limit = Arc::new(AtomicUsize::new(options.output_limit.unwrap_or(usize::MAX)));
    let mut workers = JoinSet::new();
    let wait_child = child.clone();

    workers.spawn(async move { WorkerResult::Child(wait_child.wait_managed().await) });
    workers.spawn(read_output(stdout, limit.clone(), true));
    workers.spawn(read_output(stderr, limit, false));

    if let Some(input) = input {
        let stdin = child
            .take_stdin()
            .await
            .ok_or_else(|| ProcessError::WriteInput {
                bin: bin.clone(),
                error: Box::new(io::Error::other("failed to open stdin")),
            })?;

        workers.spawn(async move {
            let mut stdin = stdin;
            let result = stdin.write_all(&input).await.or_else(ignore_broken_pipe);
            WorkerResult::Stdin(result)
        });
    }

    let execution_deadline = options.timeout.and_then(deadline_after);
    let mut exit = None;
    let mut stdout = None;
    let mut stderr = None;
    let mut failure = None;

    while exit.is_none() && failure.is_none() {
        match next_worker(&mut workers, execution_deadline).await {
            Ok(Some(result)) => apply_worker(
                result,
                &mut exit,
                &mut stdout,
                &mut stderr,
                &mut failure,
                &bin,
                options.output_limit,
            ),
            Ok(None) => {
                failure = Some(ProcessError::Capture {
                    bin: bin.clone(),
                    error: Box::new(io::Error::other(
                        "process workers exited before the child completed",
                    )),
                });
            }
            Err(()) => {
                failure = Some(ProcessError::Timeout {
                    bin: bin.clone(),
                    timeout: options.timeout.expect("timeout has a deadline"),
                    output: None,
                });
            }
        }
    }

    if failure.is_some() {
        match child.kill_with_signal(SignalType::Kill).await {
            Ok(killed) => exit = Some(killed),
            Err(error) => {
                failure = Some(ProcessError::Cleanup {
                    bin: bin.clone(),
                    error: Box::new(error),
                    failure: Box::new(failure.take().expect("cleanup follows a failure")),
                    output: None,
                });
            }
        }
    }

    let drain_deadline = options.output_drain_timeout.and_then(deadline_after);

    while stdout.is_none() || stderr.is_none() {
        match next_worker(&mut workers, drain_deadline).await {
            Ok(Some(result)) => apply_worker(
                result,
                &mut exit,
                &mut stdout,
                &mut stderr,
                &mut failure,
                &bin,
                options.output_limit,
            ),
            Ok(None) => break,
            Err(()) => {
                if failure.is_none() {
                    failure = Some(ProcessError::OutputDrainTimeout {
                        bin: bin.clone(),
                        timeout: options
                            .output_drain_timeout
                            .expect("drain timeout has a deadline"),
                        output: None,
                    });
                }
                break;
            }
        }
    }

    workers.abort_all();

    while workers.join_next().await.is_some() {}

    let output = Output {
        exit: exit.unwrap_or(ChildExit::Killed),
        stdout: stdout.unwrap_or_default(),
        stderr: stderr.unwrap_or_default(),
    };

    if let Some(mut failure) = failure {
        attach_partial_output(&mut failure, output);
        return Err(failure);
    }

    Ok(output)
}

fn apply_worker(
    result: Result<WorkerResult, tokio::task::JoinError>,
    exit: &mut Option<ChildExit>,
    stdout: &mut Option<Bytes>,
    stderr: &mut Option<Bytes>,
    failure: &mut Option<ProcessError>,
    bin: &str,
    output_limit: Option<usize>,
) {
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            set_failure(
                failure,
                ProcessError::Capture {
                    bin: bin.to_owned(),
                    error: Box::new(io::Error::other(format!(
                        "process worker panicked: {error}"
                    ))),
                },
            );
            return;
        }
    };

    match result {
        WorkerResult::Child(result) => match result {
            Ok(value) => *exit = Some(value),
            Err(error) => {
                set_failure(
                    failure,
                    ProcessError::Capture {
                        bin: bin.to_owned(),
                        error: Box::new(error),
                    },
                );
            }
        },
        WorkerResult::Stdin(result) => {
            if let Err(error) = result {
                set_failure(
                    failure,
                    ProcessError::WriteInput {
                        bin: bin.to_owned(),
                        error: Box::new(error),
                    },
                );
            }
        }
        WorkerResult::Stdout(result) => match result {
            Ok(value) => *stdout = Some(value),
            Err(error) => {
                *stdout = Some(error.output);
                set_failure(failure, capture_io_error(bin, output_limit, error.error));
            }
        },
        WorkerResult::Stderr(result) => match result {
            Ok(value) => *stderr = Some(value),
            Err(error) => {
                *stderr = Some(error.output);
                set_failure(failure, capture_io_error(bin, output_limit, error.error));
            }
        },
    }
}

fn set_failure(failure: &mut Option<ProcessError>, error: ProcessError) {
    if failure.is_none() {
        *failure = Some(error);
    }
}

fn capture_io_error(bin: &str, output_limit: Option<usize>, error: io::Error) -> ProcessError {
    if error.kind() == io::ErrorKind::FileTooLarge {
        ProcessError::OutputLimitExceeded {
            bin: bin.to_owned(),
            limit: output_limit.expect("output limit generated a limit error"),
            output: None,
        }
    } else {
        ProcessError::Capture {
            bin: bin.to_owned(),
            error: Box::new(error),
        }
    }
}

async fn next_worker(
    workers: &mut JoinSet<WorkerResult>,
    deadline: Option<TokioInstant>,
) -> Result<Option<Result<WorkerResult, tokio::task::JoinError>>, ()> {
    match deadline {
        Some(deadline) => timeout_at(deadline, workers.join_next())
            .await
            .map_err(|_| ()),
        None => Ok(workers.join_next().await),
    }
}

async fn read_output(
    mut reader: impl AsyncRead + Unpin,
    remaining: Arc<AtomicUsize>,
    stdout: bool,
) -> WorkerResult {
    let mut buffer = vec![0; READ_BUFFER_SIZE];
    let mut bytes = BytesMut::new();

    let result = loop {
        let count = match reader.read(&mut buffer).await {
            Ok(count) => count,
            Err(error) => break Err(error),
        };

        if count == 0 {
            break Ok(bytes.split().freeze());
        }

        if let Err(error) = reserve_output(&remaining, count) {
            break Err(error);
        }

        bytes.extend_from_slice(&buffer[..count]);
    }
    .map_err(|error| ReadFailure {
        error,
        output: bytes.freeze(),
    });

    if stdout {
        WorkerResult::Stdout(result)
    } else {
        WorkerResult::Stderr(result)
    }
}

fn attach_partial_output(error: &mut ProcessError, output: Output) {
    match error {
        ProcessError::OutputLimitExceeded {
            output: partial, ..
        }
        | ProcessError::OutputDrainTimeout {
            output: partial, ..
        }
        | ProcessError::Timeout {
            output: partial, ..
        }
        | ProcessError::Cleanup {
            output: partial, ..
        } => *partial = Some(output),
        _ => {}
    }
}

fn reserve_output(remaining: &AtomicUsize, count: usize) -> io::Result<()> {
    remaining
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |remaining| {
            remaining.checked_sub(count)
        })
        .map(|_| ())
        .map_err(|_| io::Error::from(io::ErrorKind::FileTooLarge))
}

fn deadline_after(duration: Duration) -> Option<TokioInstant> {
    TokioInstant::now().checked_add(duration)
}

fn ignore_broken_pipe(error: io::Error) -> io::Result<()> {
    if error.kind() == io::ErrorKind::BrokenPipe {
        Ok(())
    } else {
        Err(error)
    }
}
