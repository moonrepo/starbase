use crate::output::Output;
use crate::process_set::SetInner;
use crate::signal::*;
use bytes::Bytes;
use parking_lot::Mutex;
use process_wrap::tokio::ChildWrapper;
use std::future::{Future, poll_fn};
use std::io;
use std::pin::pin;
use std::process::ExitStatus;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout};
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
/// clone is visible to all others.
///
/// Only the original handle returned by
/// [`ProcessSet::spawn`](crate::ProcessSet::spawn) acts on drop; clones
/// never do. Dropping it kills the child if it's still running (its whole
/// tree when contained), stops output readers on its clones, and stops
/// tracking the child in its set. Call [`Self::stop_cleanup`] first to
/// leave the child running and its output readers alone.
pub struct SharedChild {
    inner: Arc<Mutex<Process>>,
    signal: Arc<OnceLock<SignalType>>,
    pid: u32,
    waiter: Arc<AsyncMutex<()>>,

    // Whether the child runs in a process group or job object of its own
    contained: bool,

    output_stop: watch::Sender<bool>,
    exited: watch::Sender<bool>,
    cleanup: Arc<AtomicBool>,

    // How many shutdowns that are still waiting include this child
    shutdowns: Arc<AtomicUsize>,

    // The set tracking this child, held by the original handle only. It's
    // a strong reference, so a set can't be dropped out from under children
    // it still tracks, which would hide them from shutdowns.
    set: Option<Arc<SetInner>>,
}

impl SharedChild {
    /// Wrap a spawned child so it can be shared across tasks.
    pub fn new(child: Child) -> Self {
        Self::wrap(Box::new(child), false)
    }

    fn wrap(child: Box<dyn ChildWrapper>, contained: bool) -> Self {
        Self {
            pid: child.id().unwrap(),
            inner: Arc::new(Mutex::new(Process(child))),
            signal: Arc::new(OnceLock::new()),
            contained,
            waiter: Arc::new(AsyncMutex::new(())),
            output_stop: watch::channel(false).0,
            exited: watch::channel(false).0,
            cleanup: Arc::new(AtomicBool::new(true)),
            shutdowns: Arc::new(AtomicUsize::new(0)),
            set: None,
        }
    }

    /// Wrap a spawned child as the owner tracked by `set`. Dropping it kills
    /// the child if it's still running, and stops tracking it either way.
    pub(crate) fn new_owned(
        child: Box<dyn ChildWrapper>,
        contained: bool,
        set: Arc<SetInner>,
    ) -> Self {
        let mut shared_child = Self::wrap(child, contained);
        shared_child.set = Some(set);
        shared_child
    }

    /// Return the child's process id.
    pub fn id(&self) -> u32 {
        self.pid
    }

    /// Return whether the child runs in a process group (Unix) or job
    /// object (Windows) of its own, so that signals reach its whole tree.
    /// See [`ProcessSet::spawn_contained`](crate::ProcessSet::spawn_contained).
    pub fn is_contained(&self) -> bool {
        self.contained
    }

    // Unlike comparing pids, this can't be fooled by the OS giving a new
    // process the pid of one that has already exited
    pub(crate) fn same_child(&self, other: &SharedChild) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    // Count a shutdown that waits on this child. Returns true for the first
    // of any that overlap, which is the one to signal it.
    pub(crate) fn join_shutdown(&self) -> bool {
        self.shutdowns.fetch_add(1, Ordering::AcqRel) == 0
    }

    pub(crate) fn leave_shutdown(&self) {
        self.shutdowns.fetch_sub(1, Ordering::AcqRel);
    }

    // Wait for a child that was just force killed to exit. This polls rather
    // than queueing behind `wait`, whose holder may be on a runtime that
    // isn't running.
    pub(crate) async fn wait_after_kill(&self) {
        loop {
            let status = self.try_wait_locked(self.inner.lock().child());

            if !matches!(status, Ok(None)) {
                return;
            }

            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    /// Take the child's stdin pipe, if it was piped and not already taken.
    ///
    /// Dropping the returned handle closes the pipe, which the child sees
    /// as end of input. That is usually what you want once all input has
    /// been written.
    pub async fn take_stdin(&self) -> Option<ChildStdin> {
        self.inner.lock().child().stdin.take()
    }

    /// Take the child's stdout pipe, if it was piped and not already taken.
    ///
    /// Keep the returned handle alive for as long as the child may write.
    /// Dropping it closes our end of the pipe, and the child is killed by
    /// `SIGPIPE` on its next write, truncating its output partway through.
    /// [`Self::wait_with_output`] returns empty bytes for a pipe taken here.
    pub async fn take_stdout(&self) -> Option<ChildStdout> {
        self.inner.lock().child().stdout.take()
    }

    /// Take the child's stderr pipe, if it was piped and not already taken.
    ///
    /// The same `SIGPIPE` caveat as [`Self::take_stdout`] applies.
    pub async fn take_stderr(&self) -> Option<ChildStderr> {
        self.inner.lock().child().stderr.take()
    }

    /// Force kill the child immediately (`SIGKILL` on Unix, terminate on
    /// Windows), stop capturing output, and wait for it to exit.
    /// Bytes already captured are retained; unread output may be truncated.
    pub async fn kill(&self) -> io::Result<ChildExit> {
        self.kill_with_signal(SignalType::Kill).await
    }

    /// Send `signal` to the child and wait for it to exit. On Windows, the
    /// signal is remembered because its exit status does not carry it.
    /// An already reaped child retains its exit status, and isn't
    /// signalled, unless it's contained, when the processes it left behind
    /// still are. `Kill` also stops capture readers, including after the
    /// child was reaped.
    pub async fn kill_with_signal(&self, signal: SignalType) -> io::Result<ChildExit> {
        let completed = {
            // Reaping and signal delivery use the same lock. Until reaped,
            // the child retains its PID/handle even if it has already exited.
            let mut process = self.inner.lock();

            self.send_signal_locked(&mut process, signal)?
                .map(|status| convert_exit_status(status, None))
        };

        if let Some(exit) = completed {
            return Ok(exit);
        }

        self.wait().await
    }

    /// Send a signal without waiting for the child. This is used by
    /// synchronous cleanup paths, such as an owner being dropped.
    pub fn send_signal(&self, signal: SignalType) -> io::Result<()> {
        let mut process = self.inner.lock();

        self.send_signal_locked(&mut process, signal)?;

        Ok(())
    }

    /// Check the live child and send a signal while its process identity is
    /// protected by the child lock. Returns an exit status if the child was
    /// already reaped, so callers can preserve the real result.
    fn send_signal_locked(
        &self,
        process: &mut Process,
        signal: SignalType,
    ) -> io::Result<Option<ExitStatus>> {
        let status = self.try_wait_locked(process.child())?;

        if status.is_none() {
            process.signal(signal, self.contained)?;

            // `CTRL-C` cannot be delivered to an individual Windows process, so
            // do not report it as an interrupt when the process exits normally.
            if cfg!(unix) || !matches!(signal, SignalType::Interrupt) {
                self.signal.get_or_init(|| signal);
            }
        } else if self.contained {
            // The processes it started may still be running, and holding
            // its output pipes open
            process.signal(signal, true)?;
        }

        if matches!(signal, SignalType::Kill) {
            self.stop_output();
        }

        Ok(status)
    }

    /// Reaping through `try_wait` makes tokio drop its `SIGCHLD` listener
    /// for the child, which a concurrent [`Self::wait`] relies on to be
    /// woken. Record the exit so those waiters are woken instead.
    fn try_wait_locked(&self, child: &mut Child) -> io::Result<Option<ExitStatus>> {
        let status = child.try_wait()?;

        if status.is_some() {
            self.exited.send_replace(true);
        }

        Ok(status)
    }

    /// Stop the original handle from killing the child, or stopping output
    /// readers, when it's dropped. Dropping it still stops tracking the
    /// child, so shutdowns no longer reach it. This can be called through
    /// any clone.
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
        let mut exited = self.exited.subscribe();

        let status = tokio::select! {
            biased;
            status = poll_fn(|cx| {
                let mut process = self.inner.lock();
                pin!(process.child().wait()).poll(cx)
            }) => status?,
            // The child was reaped by a `try_wait` elsewhere, taking tokio's
            // `SIGCHLD` listener with it, so the wait above would never be
            // woken. Its exit status is retained. The watch guard is dropped
            // before locking the child, which `try_wait_locked` holds while
            // it updates the watch.
            _ = async { let _ = exited.wait_for(|exited| *exited).await; } => {
                self.inner.lock().child().try_wait()?.ok_or_else(|| {
                    io::Error::other("Child process was marked exited but has no status")
                })?
            }
        };

        Ok(convert_exit_status(status, self.signal.get().copied()))
    }

    /// Wait for the child to exit and drain its piped output.
    ///
    /// This returns once the pipes reach end of file, which is not always
    /// when the child exits. Any process that inherited the pipes holds
    /// them open, so a shell wrapper that backgrounds work keeps us here
    /// until that work finishes, and its output is captured too. To bound
    /// the wait, contain the child, so signals reach its whole tree, or
    /// make the process being signalled the one holding the pipes (`exec`
    /// in a shell wrapper), as signalling an uncontained shell does not
    /// reach the processes it spawned. Force-killing stops the capture
    /// readers and returns bytes already captured, without waiting for
    /// inherited pipes. Unless the child is contained, it does not
    /// terminate descendants.
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
            let mut process = self.inner.lock();
            let child = process.child();

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
            contained: self.contained,
            waiter: Arc::clone(&self.waiter),
            output_stop: self.output_stop.clone(),
            exited: self.exited.clone(),
            cleanup: Arc::clone(&self.cleanup),
            shutdowns: Arc::clone(&self.shutdowns),
            // Only the original instance has access to the set!
            set: None,
        }
    }
}

impl Drop for SharedChild {
    fn drop(&mut self) {
        if let Some(set) = self.set.take() {
            // A child still running when its owner goes away was cancelled,
            // so kill it now rather than queueing that for later. Processes
            // left behind by a child that already exited keep running.
            if self.cleanup.swap(false, Ordering::AcqRel) {
                let mut process = self.inner.lock();

                let result = match self.try_wait_locked(process.child()) {
                    Ok(None) => self.send_signal_locked(&mut process, SignalType::Kill),
                    Ok(Some(status)) => {
                        self.stop_output();

                        Ok(Some(status))
                    }
                    Err(error) => Err(error),
                };

                if let Err(error) = result {
                    warn!(
                        pid = self.pid,
                        %error,
                        "Failed to kill cancelled child process"
                    );
                }
            }

            set.remove(self);
        }
    }
}

// The spawned child, along with the process group or job object wrapper
// that contains its tree, if any
struct Process(Box<dyn ChildWrapper>);

impl Process {
    // The native child, which is waited on directly. The wrappers' own waits
    // also wait for the rest of the tree, which `SharedChild::wait` doesn't.
    fn child(&mut self) -> &mut Child {
        // SAFETY: The process group and job object wrappers only cache the
        // exit status seen by their own waits, which are never called
        unsafe { self.0.try_inner_child_mut() }.expect("Child process wrapper has no native child")
    }

    // Send `signal` to the child, or to its whole tree when contained
    fn signal(&mut self, signal: SignalType, contained: bool) -> io::Result<()> {
        #[cfg(unix)]
        {
            if contained {
                // The whole process group has exited
                return match self.0.signal(signal.get_code()) {
                    Err(error) if error.raw_os_error() == Some(libc::ESRCH) => Ok(()),
                    result => result,
                };
            }

            if let Some(pid) = self.child().id() {
                kill(pid, signal)?;
            }
        }

        #[cfg(windows)]
        {
            // `CTRL-C` is delivered by the console, and can't be sent to
            // a single process or job. Everything else terminates.
            if contained {
                if !matches!(signal, SignalType::Interrupt) {
                    self.0.start_kill()?;
                }

                return Ok(());
            }

            let child = self.child();

            // Borrow the live handle only while the lock prevents reaping
            // from closing it; never retain a raw handle across waits.
            if let (Some(pid), Some(handle)) = (child.id(), child.raw_handle()) {
                kill(pid, RawHandle(handle), signal)?;
            }
        }

        Ok(())
    }
}

fn convert_exit_status(status: ExitStatus, _raw_signal: Option<SignalType>) -> ChildExit {
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

    #[cfg(not(unix))]
    {
        if let Some(signal) = _raw_signal {
            return match signal {
                SignalType::Interrupt => ChildExit::Interrupted,
                SignalType::Kill => ChildExit::Killed,
                other => ChildExit::Terminated(other.get_code()),
            };
        }
    }

    ChildExit::Completed(status)
}
