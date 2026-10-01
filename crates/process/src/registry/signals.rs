//! Opt-in OS signal listeners and registry shutdown integration.

use super::process::signal_processes;
use super::{ProcessRegistry, RegistryEvent, RegistryState, lock};
use crate::SignalType;
use std::io;
use std::sync::Arc;

impl ProcessRegistry {
    /// Install opt-in OS signal handling. A received signal is broadcast and
    /// initiates graceful shutdown; another signal during shutdown forces a kill.
    /// Listener installation errors are returned. Multiple calls are idempotent.
    /// Tokio's OS signal handler persists even after a listener is dropped.
    pub fn listen_for_signals(&self) -> io::Result<()> {
        let runtime = tokio::runtime::Handle::try_current().map_err(io::Error::other)?;
        let mut listener = lock(&self.owner.listener);
        if listener.as_ref().is_some_and(|task| !task.is_finished()) {
            return Ok(());
        }
        let mut signals = OsSignals::new()?;
        let core = Arc::downgrade(&self.owner.core);
        *listener = Some(runtime.spawn(async move {
            while let Some(signal) = signals.recv().await {
                let Some(core) = core.upgrade() else { break };
                let _ = core.events.send(RegistryEvent::SignalReceived(signal));
                if lock(&core.state).phase == RegistryState::ShuttingDown {
                    let _ = signal_processes(lock(&core.state).running.values(), SignalType::Kill);
                } else if let Err(error) = core.begin_shutdown(signal) {
                    tracing::warn!(%error, "Failed to begin process shutdown after OS signal");
                }
            }
        }));
        Ok(())
    }

    /// Stop this registry's signal listener. Does not undo Tokio's process-wide
    /// signal installation, change admissions, or stop process supervisors.
    pub fn stop_signal_listener(&self) {
        if let Some(task) = lock(&self.owner.listener).take() {
            task.abort();
        }
    }
}
#[cfg(unix)]
struct OsSignals {
    interrupt: tokio::signal::unix::Signal,
    terminate: tokio::signal::unix::Signal,
    quit: tokio::signal::unix::Signal,
}

#[cfg(unix)]
impl OsSignals {
    fn new() -> io::Result<Self> {
        use tokio::signal::unix::{SignalKind, signal};
        Ok(Self {
            interrupt: signal(SignalKind::interrupt())?,
            terminate: signal(SignalKind::terminate())?,
            quit: signal(SignalKind::quit())?,
        })
    }

    async fn recv(&mut self) -> Option<SignalType> {
        tokio::select! {
            result = self.interrupt.recv() => result.map(|_| SignalType::Interrupt),
            result = self.terminate.recv() => result.map(|_| SignalType::Terminate),
            result = self.quit.recv() => result.map(|_| SignalType::Quit),
        }
    }
}

#[cfg(windows)]
struct OsSignals {
    interrupt: tokio::signal::windows::CtrlC,
    brk: tokio::signal::windows::CtrlBreak,
    close: tokio::signal::windows::CtrlClose,
    shutdown: tokio::signal::windows::CtrlShutdown,
}

#[cfg(windows)]
impl OsSignals {
    fn new() -> io::Result<Self> {
        use tokio::signal::windows::*;
        Ok(Self {
            interrupt: ctrl_c()?,
            brk: ctrl_break()?,
            close: ctrl_close()?,
            shutdown: ctrl_shutdown()?,
        })
    }

    async fn recv(&mut self) -> Option<SignalType> {
        tokio::select! {
            result = self.interrupt.recv() => result.map(|_| SignalType::Interrupt),
            result = self.brk.recv() => result.map(|_| SignalType::Interrupt),
            result = self.close.recv() => result.map(|_| SignalType::Quit),
            result = self.shutdown.recv() => result.map(|_| SignalType::Terminate),
        }
    }
}
