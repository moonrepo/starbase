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
            pid: child.id().unwrap(),
            inner: Arc::new(Mutex::new(child)),
            signal: Arc::new(OnceLock::new()),
        }
    }

    /// Wrap a spawned child so it can be shared across tasks.
    #[cfg(windows)]
    pub fn new(child: Child) -> Self {
        Self {
            pid: child.id().unwrap(),
            handle: RawHandle(child.raw_handle().unwrap()),
            inner: Arc::new(Mutex::new(child)),
            signal: Arc::new(OnceLock::new()),
        }
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
        let mut child = self.inner.lock().await;

        child.kill().await?;

        Ok(ChildExit::Killed)
    }

    /// Send `signal` to the child and wait for it to exit. The signal is
    /// remembered, so the resulting [`ChildExit`] reflects it even if the
    /// child's own exit status doesn't carry it (e.g. on Windows).
    pub async fn kill_with_signal(&self, signal: SignalType) -> io::Result<ChildExit> {
        self.signal.get_or_init(|| signal);

        #[cfg(unix)]
        {
            kill(self.pid, signal)?;
        }

        #[cfg(windows)]
        {
            kill(self.pid, self.handle.clone(), signal)?;
        }

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
