//! Registry ownership, admission state, and coordinated shutdown.

use super::cache::CacheEntry;
use super::process::{Process, signal_processes};
use super::{ProcessId, RegistryEvent, RegistryOptions, RegistryState, lock, shared_error};
use crate::SignalType;
use std::collections::{HashMap, VecDeque};
use std::io;
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;

type ShutdownResult = Result<(), Arc<io::Error>>;
type ShutdownReceiver = watch::Receiver<Option<ShutdownResult>>;

pub(super) struct Owner {
    pub(super) core: Arc<Core>,
    pub(super) listener: Mutex<Option<JoinHandle<()>>>,
}

pub(super) struct Core {
    pub(super) options: RegistryOptions,
    pub(super) state: Mutex<State>,
    pub(super) events: broadcast::Sender<RegistryEvent>,
    pub(super) changes: watch::Sender<u64>,
}

pub(super) struct State {
    pub(super) phase: RegistryState,
    pub(super) next_id: u64,
    pub(super) running: HashMap<ProcessId, Arc<Process>>,
    pub(super) cache: VecDeque<CacheEntry>,
    pub(super) cache_bytes: usize,
    pub(super) cache_epoch: u64,
    pub(super) shutdown: Option<ShutdownReceiver>,
}

impl Drop for Owner {
    fn drop(&mut self) {
        if let Some(task) = lock(&self.listener).take() {
            task.abort();
        }
        let state = lock(&self.core.state);
        if let Err(error) = signal_processes(state.running.values(), SignalType::Kill) {
            tracing::warn!(%error, "Failed to kill processes while dropping registry");
        }
    }
}

impl Core {
    pub(super) async fn wait_for_idle(&self) {
        let mut changes = self.changes.subscribe();
        loop {
            if lock(&self.state).running.is_empty() {
                return;
            }
            if changes.changed().await.is_err() {
                return;
            }
        }
    }

    pub(super) fn begin_shutdown(
        self: &Arc<Self>,
        initial_signal: SignalType,
    ) -> io::Result<ShutdownReceiver> {
        let runtime = tokio::runtime::Handle::try_current().map_err(io::Error::other)?;
        let mut state = lock(&self.state);
        if state.phase == RegistryState::ShuttingDown {
            return state
                .shutdown
                .clone()
                .ok_or_else(|| io::Error::other("missing shutdown result"));
        }
        state.phase = RegistryState::ShuttingDown;
        let _ = self.events.send(RegistryEvent::StateChanged(state.phase));
        let (sender, receiver) = watch::channel(None);
        state.shutdown = Some(receiver.clone());
        let processes = state.running.values().cloned().collect::<Vec<_>>();
        let core = self.clone();
        let mut completion = ShutdownCompletion {
            core: core.clone(),
            sender,
            finished: false,
        };
        runtime.spawn(async move {
            // Windows has no targeted graceful signal API. OS interrupts may
            // arrive through the shared console; explicitly shut down the job.
            #[cfg(windows)]
            let initial_signal = if matches!(initial_signal, SignalType::Interrupt) {
                SignalType::Terminate
            } else {
                initial_signal
            };
            let mut result = signal_processes(processes.iter(), initial_signal);
            if tokio::time::timeout(core.options.shutdown_grace, core.wait_for_idle())
                .await
                .is_err()
            {
                match signal_processes(processes.iter(), SignalType::Kill) {
                    Ok(()) => core.wait_for_idle().await,
                    Err(error) => {
                        // Keep supervision and report failed termination instead of
                        // waiting forever on a process we could not kill.
                        completion.finish(Err(error));
                        return;
                    }
                }
            }
            for process in &processes {
                if let Some(Err(error)) = process.result.borrow().as_ref()
                    && result.is_ok()
                {
                    result = Err(shared_error(error.clone()));
                }
            }
            completion.finish(result);
        });
        Ok(receiver)
    }
}

struct ShutdownCompletion {
    core: Arc<Core>,
    sender: watch::Sender<Option<ShutdownResult>>,
    finished: bool,
}

impl ShutdownCompletion {
    fn finish(&mut self, result: io::Result<()>) {
        let mut state = lock(&self.core.state);
        state.phase = RegistryState::Stopped;
        state.shutdown = None;
        self.sender.send_replace(Some(result.map_err(Arc::new)));
        let _ = self
            .core
            .events
            .send(RegistryEvent::StateChanged(state.phase));
        self.finished = true;
    }
}

impl Drop for ShutdownCompletion {
    fn drop(&mut self) {
        if !self.finished {
            self.finish(Err(io::Error::other("shutdown supervisor was cancelled")));
        }
    }
}
