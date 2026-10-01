//! Shared process ownership, durable result handles, and tree signalling.

#[cfg(target_vendor = "apple")]
use super::unix::apple_group_has_only_zombies;
use super::{ProcessId, RegistryEvent, lock, shared_error};
use crate::{Output, SharedChild, SignalType};
use std::io;
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, watch};

type Completion = Result<Output, Arc<io::Error>>;

/// A durable, cloneable result handle. Dropping it does not cancel the process.
/// Use `signal` to terminate it, or shut down the registry that owns it.
#[derive(Clone)]
pub struct ProcessHandle {
    pub(super) process: Arc<Process>,
    result: watch::Receiver<Option<Completion>>,
}

impl ProcessHandle {
    /// Registry-local process identity.
    pub fn id(&self) -> ProcessId {
        self.process.id
    }

    /// Original OS process ID, for diagnostics only.
    pub fn pid(&self) -> u32 {
        self.process.child.id()
    }

    /// Send a signal to the process tree. Completed processes are a no-op.
    /// Windows does not support targeted interrupts and returns `Unsupported`.
    pub fn signal(&self, signal: SignalType) -> io::Result<()> {
        self.process.signal(signal)
    }

    /// Wait for supervision and I/O to finish. Cancellation does not lose output;
    /// multiple callers may wait and each receives the same result.
    pub async fn wait(&self) -> io::Result<Output> {
        let mut receiver = self.result.clone();
        loop {
            if let Some(result) = receiver.borrow_and_update().as_ref() {
                return result.clone().map_err(shared_error);
            }
            receiver.changed().await.map_err(|_| {
                io::Error::other("process supervisor stopped without a completion result")
            })?;
        }
    }
}

pub(super) struct Process {
    pub(super) id: ProcessId,
    pub(super) child: SharedChild,
    // On Unix this remains true until group cleanup BEFORE reaping, pinning the
    // group leader PID against reuse. No public API exposes the SharedChild.
    pub(super) active: Mutex<bool>,
    pub(super) result: watch::Sender<Option<Completion>>,
    pub(super) events: broadcast::Sender<RegistryEvent>,
    #[cfg(windows)]
    pub(super) job: super::windows::Job,
}

impl Process {
    pub(super) fn handle(self: &Arc<Self>) -> ProcessHandle {
        ProcessHandle {
            process: self.clone(),
            result: self.result.subscribe(),
        }
    }

    pub(super) fn signal(&self, signal: SignalType) -> io::Result<()> {
        let active = lock(&self.active);
        if !*active {
            return Ok(());
        }
        self.signal_tree(signal)?;
        if matches!(signal, SignalType::Kill) {
            self.child.stop_output();
        }
        let _ = self.events.send(RegistryEvent::Signalled {
            id: self.id,
            signal,
        });
        Ok(())
    }

    fn signal_tree(&self, signal: SignalType) -> io::Result<()> {
        #[cfg(unix)]
        {
            // SAFETY: spawn placed the child in its own group (PGID == PID).
            // The active lock prevents reaping until all group signals finish.
            let result = unsafe { libc::kill(-(self.child.id() as i32), signal.get_code()) };
            if result == -1 {
                let error = io::Error::last_os_error();
                if error.raw_os_error() == Some(libc::ESRCH) {
                    return Ok(());
                }
                // Darwin reports EPERM for a group consisting entirely of
                // zombies. Verify that condition without hiding real denials.
                #[cfg(target_vendor = "apple")]
                if error.raw_os_error() == Some(libc::EPERM)
                    && apple_group_has_only_zombies(self.child.id())?
                {
                    return Ok(());
                }
                return Err(error);
            }
            Ok(())
        }
        #[cfg(windows)]
        {
            if matches!(signal, SignalType::Interrupt) {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "targeted Windows interrupts are unsupported",
                ));
            }
            self.job.terminate()
        }
    }

    pub(super) fn finish_tree(&self) -> io::Result<()> {
        let mut active = lock(&self.active);
        if *active {
            self.signal_tree(SignalType::Kill)?;
            *active = false;
        }
        Ok(())
    }
}

pub(super) fn signal_processes<'a>(
    processes: impl Iterator<Item = &'a Arc<Process>>,
    signal: SignalType,
) -> io::Result<()> {
    let mut first_error = None;
    for process in processes {
        if let Err(error) = process.signal(signal) {
            first_error.get_or_insert(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}
