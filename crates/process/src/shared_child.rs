use crate::output::Output;
use crate::signal::*;
use bytes::Bytes;
use std::future::{Future, poll_fn};
use std::io;
use std::pin::pin;
use std::process::ExitStatus;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout};
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::{Mutex as AsyncMutex, watch};
use tracing::warn;

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
/// clone is visible to all others. Only the original handle created with
/// [`Self::new_with_cleanup`] requests cleanup on drop; clones never do.
pub struct SharedChild {
    inner: Arc<Mutex<Child>>,
    signal: Arc<OnceLock<SignalType>>,
    pid: u32,
    waiter: Arc<AsyncMutex<()>>,

    output_stop: watch::Sender<bool>,
    cleanup: Arc<AtomicBool>,
    cleanup_sender: Option<UnboundedSender<SharedChild>>,
}

impl SharedChild {
    /// Wrap a spawned child so it can be shared across tasks.
    pub fn new(child: Child) -> Self {
        Self {
            pid: child.id().unwrap(),
            inner: Arc::new(Mutex::new(child)),
            signal: Arc::new(OnceLock::new()),
            waiter: Arc::new(AsyncMutex::new(())),
            output_stop: watch::channel(false).0,
            cleanup: Arc::new(AtomicBool::new(true)),
            cleanup_sender: None,
        }
    }

    /// Wrap a spawned child so it can be shared across tasks, and register
    /// a cleanup sender to be notified when this handle is dropped.
    /// Clones do not inherit responsibility for cleanup.
    pub fn new_with_cleanup(child: Child, cleanup_sender: UnboundedSender<SharedChild>) -> Self {
        let mut shared_child = Self::new(child);
        shared_child.cleanup_sender = Some(cleanup_sender);
        shared_child
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
        self.inner.lock().unwrap().stdin.take()
    }

    /// Take the child's stdout pipe, if it was piped and not already taken.
    ///
    /// Keep the returned handle alive for as long as the child may write.
    /// Dropping it closes our end of the pipe, and the child is killed by
    /// `SIGPIPE` on its next write, truncating its output partway through.
    /// [`Self::wait_with_output`] returns empty bytes for a pipe taken here.
    pub async fn take_stdout(&self) -> Option<ChildStdout> {
        self.inner.lock().unwrap().stdout.take()
    }

    /// Take the child's stderr pipe, if it was piped and not already taken.
    ///
    /// The same `SIGPIPE` caveat as [`Self::take_stdout`] applies.
    pub async fn take_stderr(&self) -> Option<ChildStderr> {
        self.inner.lock().unwrap().stderr.take()
    }

    /// Force kill the child immediately (`SIGKILL` on Unix, terminate on
    /// Windows), stop capturing output, and wait for it to exit.
    /// Bytes already captured are retained; unread output may be truncated.
    pub async fn kill(&self) -> io::Result<ChildExit> {
        // Tokio checks its cached exit state before starting a kill.
        self.inner.lock().unwrap().start_kill()?;
        self.stop_output();
        self.wait().await?;

        Ok(ChildExit::Killed)
    }

    /// Send `signal` to the child and wait for it to exit. The signal is
    /// remembered, so the resulting [`ChildExit`] reflects it even if the
    /// child's own exit status doesn't carry it (e.g. on Windows).
    /// An already reaped child is not signalled and retains its exit status.
    /// `Kill` also stops capture readers, including after the child was reaped.
    pub async fn kill_with_signal(&self, signal: SignalType) -> io::Result<ChildExit> {
        let completed = {
            // Reaping and signal delivery use the same lock. Until reaped,
            // the child retains its PID/handle even if it has already exited.
            let mut child = self.inner.lock().unwrap();

            self.send_signal_locked(&mut child, signal)?
                .map(|status| convert_exit_status(status, None))
        };

        if let Some(exit) = completed {
            return Ok(exit);
        }

        self.wait().await
    }

    /// Send a signal without waiting for the child. This is used by
    /// synchronous cleanup paths, such as a registry being dropped.
    pub fn send_signal(&self, signal: SignalType) -> io::Result<()> {
        let mut child = self.inner.lock().unwrap();

        self.send_signal_locked(&mut child, signal)?;

        Ok(())
    }

    /// Check the live child and send a signal while its process identity is
    /// protected by the child lock. Returns an exit status if the child was
    /// already reaped, so callers can preserve the real result.
    fn send_signal_locked(
        &self,
        child: &mut Child,
        signal: SignalType,
    ) -> io::Result<Option<ExitStatus>> {
        if let Some(status) = child.try_wait()? {
            if matches!(signal, SignalType::Kill) {
                self.stop_output();
            }

            return Ok(Some(status));
        }

        let Some(pid) = child.id() else {
            return Ok(None);
        };

        #[cfg(unix)]
        kill(pid, signal)?;

        #[cfg(windows)]
        {
            // Borrow the live handle only while the lock prevents reaping
            // from closing it; never retain a raw handle across waits.
            let handle = child
                .raw_handle()
                .ok_or_else(|| io::Error::other("Child process handle is unavailable"))?;

            kill(pid, RawHandle(handle), signal)?;
        }

        self.signal.get_or_init(|| signal);

        if matches!(signal, SignalType::Kill) {
            self.stop_output();
        }

        Ok(None)
    }

    /// Disable the original handle's cleanup on drop. This can be called
    /// through any clone when the child is explicitly unregistered.
    pub fn stop_cleanup(&self) {
        self.cleanup.store(false, Ordering::Release);
    }

    /// Stop reading from the child's output pipes, so any readers return
    /// EOF. This is used to avoid deadlocks when the child is killed and
    /// its output is no longer needed. It does not terminate the child.
    pub fn stop_output(&self) {
        self.output_stop.send_replace(true);
    }

    /// Wait for the child to exit, mapping a terminating signal onto the
    /// matching [`ChildExit`] variant.
    ///
    /// This returns as soon as the child exits, and does not wait on any
    /// process it may have spawned. Unlike [`Self::wait_with_output`], no
    /// pipes are read, so a child writing to a full pipe will block forever.
    pub async fn wait(&self) -> io::Result<ChildExit> {
        // Tokio supports one waiter. Serialize wait futures, but hold the
        // child lock only during each poll so signals can still be delivered.
        let _waiter = self.waiter.lock().await;

        let status = poll_fn(|cx| {
            let mut child = self.inner.lock().unwrap();
            pin!(child.wait()).poll(cx)
        })
        .await?;

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
    /// reach the processes it spawned. Force-killing stops the capture readers
    /// and returns bytes already captured, without waiting for inherited pipes.
    /// It does not terminate descendants.
    ///
    /// Pipes that were not requested, or that [`Self::take_stdout`] and
    /// friends already took, come back as empty bytes.
    // This method re-implements the tokio `wait_with_output` method
    // but does not take ownership of self. This is required to be able
    // to call `kill`, otherwise the child does not exist.
    pub async fn wait_with_output(&self) -> io::Result<Output> {
        use tokio::{io::AsyncReadExt, try_join};

        async fn read_to_end<A: AsyncReadExt + Unpin>(
            child: &SharedChild,
            data: &mut Option<A>,
        ) -> io::Result<Vec<u8>> {
            let mut vec = Vec::new();

            if let Some(data) = data.as_mut() {
                tokio::select! {
                    biased;
                    _ = child.wait_till_output_stopped() => {},
                    result = data.read_to_end(&mut vec) => { result?; },
                }
            }

            Ok(vec)
        }

        let (mut stdout_pipe, mut stderr_pipe) = {
            let mut child = self.inner.lock().unwrap();
            (child.stdout.take(), child.stderr.take())
        };

        let stdout_fut = read_to_end(self, &mut stdout_pipe);
        let stderr_fut = read_to_end(self, &mut stderr_pipe);

        let (exit, stdout, stderr) = try_join!(self.wait(), stdout_fut, stderr_fut)?;

        drop(stdout_pipe);
        drop(stderr_pipe);

        Ok(Output {
            exit,
            stdout: Bytes::from(stdout),
            stderr: Bytes::from(stderr),
        })
    }

    /// Wait for the child to stop writing to its output pipes, which is
    /// usually when it exits or is killed.
    pub async fn wait_till_output_stopped(&self) {
        let mut receiver = self.output_stop.subscribe();
        let _ = receiver.wait_for(|stopped| *stopped).await;
    }
}

impl Clone for SharedChild {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            signal: Arc::clone(&self.signal),
            pid: self.pid,
            waiter: Arc::clone(&self.waiter),
            output_stop: self.output_stop.clone(),
            cleanup: Arc::clone(&self.cleanup),
            // Only the original instance has access to the sender!
            cleanup_sender: None,
        }
    }
}

impl Drop for SharedChild {
    fn drop(&mut self) {
        if let Some(sender) = self.cleanup_sender.take()
            && self.cleanup.swap(false, Ordering::AcqRel)
            && let Err(error) = sender.send(self.clone())
        {
            let child = error.0;
            let pid = child.id();

            if let Err(error) = child.send_signal(SignalType::Kill) {
                warn!(
                    pid,
                    %error,
                    "Failed to kill cancelled child process after registry shutdown",
                );
            }
        }
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
