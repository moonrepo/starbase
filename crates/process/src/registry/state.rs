use super::child::{ChildState, TrackedChild};
use super::event::ProcessEvent;
use super::options::ProcessRegistryOptions;
use super::tree::{self, kill_descendants};
use crate::output::Output;
use crate::shared_child::SharedChild;
use crate::signal::SignalType;
use rustc_hash::FxHashMap;
use std::io;
use std::sync::Arc;
use tokio::sync::{Mutex as AsyncMutex, Notify, RwLock, broadcast, mpsc, watch};
use tokio::task::spawn_blocking;
use tokio::time::sleep;
use tracing::{debug, trace, warn};

/// The state shared between the registry and its background tasks. It
/// outlives a [`ProcessRegistry::stop`], so tracked children, the cache,
/// and subscribers survive a restart.
pub(super) struct RegistryState {
    pub(super) options: ProcessRegistryOptions,
    pub(super) children: RwLock<FxHashMap<u32, TrackedChild>>,
    pub(super) cache: scc::HashCache<String, Output>,
    pub(super) inflight: scc::HashMap<String, watch::Receiver<Option<Output>>>,
    pub(super) events: broadcast::Sender<ProcessEvent>,
    pub(super) signals: broadcast::Sender<SignalType>,
    pub(super) running: watch::Sender<usize>,
    pub(super) reap: Notify,
    pub(super) cleanup_sender: mpsc::UnboundedSender<SharedChild>,
    pub(super) cleanup_receiver: AsyncMutex<mpsc::UnboundedReceiver<SharedChild>>,
}

impl RegistryState {
    pub(super) fn emit(&self, event: ProcessEvent) {
        let _ = self.events.send(event);
    }

    /// Publish the running count. Called with the write lock held, so a
    /// waiter woken by the change observes the new state.
    pub(super) fn sync_running_count(&self, children: &FxHashMap<u32, TrackedChild>) {
        let count = children.values().filter(|c| c.is_running()).count();

        self.running.send_replace(count);
    }

    /// Check every running child for exit, and update those that have.
    pub(super) async fn reap_exited(&self) {
        let exited = {
            let children = self.children.read().await;

            children
                .values()
                .filter(|tracked| tracked.is_running())
                .filter_map(|tracked| match tracked.child.try_wait() {
                    Ok(Some(exit)) => Some((tracked.pid(), exit)),
                    Ok(None) => None,
                    Err(error) => {
                        warn!(pid = tracked.pid(), %error, "Failed to check child process for exit");
                        None
                    }
                })
                .collect::<Vec<_>>()
        };

        if exited.is_empty() {
            return;
        }

        let mut children = self.children.write().await;

        for (pid, exit) in exited {
            // It may have been released while we weren't holding the lock
            if let Some(tracked) = children.get_mut(&pid) {
                debug!(pid, exit = ?exit, "Child process exited");

                tracked.state = ChildState::Exited(exit.clone());

                self.emit(ProcessEvent::Exited { pid, exit });
            }
        }

        self.sync_running_count(&children);
    }

    /// Wait until the given children, or every child when `None`, have
    /// exited. Reaps directly so this also works while stopped.
    pub(super) async fn wait_until_exited(&self, pids: Option<&[u32]>) {
        let mut running = self.running.subscribe();

        loop {
            self.reap_exited().await;

            let done = {
                let children = self.children.read().await;

                match pids {
                    Some(pids) => !pids
                        .iter()
                        .any(|pid| children.get(pid).is_some_and(|c| c.is_running())),
                    None => !children.values().any(|c| c.is_running()),
                }
            };

            if done {
                return;
            }

            tokio::select! {
                _ = running.changed() => {},
                _ = sleep(self.options.reap_interval) => {},
            }
        }
    }

    /// The original handle of a tracked child was dropped. A child that is
    /// still running has been cancelled, so kill it, then release it.
    pub(super) async fn cleanup_dropped(self: Arc<Self>, child: SharedChild) {
        let pid = child.id();

        let Some(tracked) = self.children.read().await.get(&pid).cloned() else {
            return;
        };

        if tracked.is_running() && self.options.kill_on_drop {
            debug!(pid, "Child process handle dropped while running; killing");

            if let Err(error) = self.signal_tree(&child, SignalType::Kill).await {
                warn!(pid, %error, "Failed to kill cancelled child process");
            }

            if let Err(error) = child.wait().await {
                warn!(pid, %error, "Failed to wait on cancelled child process");
            }
        }

        let removed = {
            let mut children = self.children.write().await;
            let removed = children.remove(&pid).is_some();

            self.sync_running_count(&children);
            removed
        };

        if removed {
            trace!(pid, "Released dropped child process");

            self.emit(ProcessEvent::Released { pid });
        }
    }

    /// Signal a child and, when configured, its descendants. The tree is
    /// snapshotted before the parent is signalled, as its exit re-parents
    /// them. An already reaped child is not signalled, but `Kill` still
    /// stops its output readers.
    pub(super) async fn signal_tree(
        &self,
        child: &SharedChild,
        signal: SignalType,
    ) -> io::Result<()> {
        let pid = child.id();

        let descendants = if self.options.signal_descendants {
            spawn_blocking(move || tree::descendants(pid))
                .await
                .unwrap_or_default()
        } else {
            vec![]
        };

        trace!(pid, ?signal, ?descendants, "Signalling child process");

        child.send_signal(signal)?;

        kill_descendants(&descendants, signal);

        Ok(())
    }

    /// Shut down the running children. Returns a signal that arrived after
    /// every child of this shutdown had exited, which starts a new one.
    pub(super) async fn perform_shutdown(
        &self,
        signal: SignalType,
        mut receiver: Option<&mut broadcast::Receiver<SignalType>>,
    ) -> Option<SignalType> {
        let targets = {
            let children = self.children.read().await;

            children
                .values()
                .filter(|c| c.is_running())
                .cloned()
                .collect::<Vec<_>>()
        };

        if targets.is_empty() {
            // Readers may still be draining pipes held open by the
            // descendants of an exited child
            if matches!(signal, SignalType::Kill) {
                self.stop_all_output().await;
            }

            self.emit(ProcessEvent::ShutdownFinished);

            return None;
        }

        let pids = targets.iter().map(|c| c.pid()).collect::<Vec<_>>();

        debug!(
            ?signal,
            ?pids,
            "Shutting down {} running child processes",
            targets.len()
        );

        self.emit(ProcessEvent::ShutdownStarted {
            signal,
            pids: pids.clone(),
        });

        for tracked in &targets {
            if let Err(error) = self.signal_tree(&tracked.child, signal).await {
                warn!(pid = tracked.pid(), %error, "Failed to signal child process");
            }
        }

        let mut pending = None;

        let force = if matches!(signal, SignalType::Kill) {
            true
        } else {
            let threshold = self.options.shutdown_threshold;

            tokio::select! {
                _ = self.wait_until_exited(Some(&pids)) => false,
                _ = sleep(threshold), if !threshold.is_zero() => {
                    debug!("Shutdown threshold exhausted");
                    true
                },
                repeated = receive_repeated_signal(&mut receiver), if receiver.is_some() => {
                    self.emit(ProcessEvent::Signal(repeated));

                    let still_running = {
                        let children = self.children.read().await;
                        pids.iter().any(|pid| children.get(pid).is_some_and(|c| c.is_running()))
                    };

                    // Every child of this shutdown has already exited, so
                    // this is a new request rather than an escalation
                    if still_running {
                        debug!(signal = ?repeated, "Received another signal during shutdown");
                        true
                    } else {
                        pending = Some(repeated);
                        false
                    }
                },
            }
        };

        if force {
            // Includes children tracked after the shutdown began
            let remaining = {
                let children = self.children.read().await;

                children
                    .values()
                    .filter(|c| c.is_running())
                    .cloned()
                    .collect::<Vec<_>>()
            };

            let pids = remaining.iter().map(|c| c.pid()).collect::<Vec<_>>();

            debug!(?pids, "Force killing {} child processes", remaining.len());

            self.emit(ProcessEvent::ShutdownForced { pids: pids.clone() });

            for tracked in &remaining {
                if let Err(error) = self.signal_tree(&tracked.child, SignalType::Kill).await {
                    warn!(pid = tracked.pid(), %error, "Failed to kill child process");
                }
            }

            self.stop_all_output().await;
            self.wait_until_exited(Some(&pids)).await;
        }

        debug!("Shutdown of child processes finished");

        self.emit(ProcessEvent::ShutdownFinished);

        pending
    }

    /// Stop the output readers of every tracked child, running or exited,
    /// so nothing waits on a pipe inherited by a descendant.
    async fn stop_all_output(&self) {
        for tracked in self.children.read().await.values() {
            tracked.child.stop_output();
        }
    }
}

async fn receive_repeated_signal(
    receiver: &mut Option<&mut broadcast::Receiver<SignalType>>,
) -> SignalType {
    match receiver.as_mut() {
        Some(receiver) => match receiver.recv().await {
            Ok(signal) => signal,
            Err(_) => SignalType::Kill,
        },
        None => std::future::pending().await,
    }
}
