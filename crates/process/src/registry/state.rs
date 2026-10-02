use super::child::{ChildSelector, ChildState, TrackedChild};
use super::event::ProcessEvent;
use super::options::ProcessRegistryOptions;
use super::tree::{self, kill_descendants};
use crate::output::Output;
use crate::shared_child::SharedChild;
use crate::signal::SignalType;
use rustc_hash::FxHashMap;
use std::io;
use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::Duration;
use tokio::sync::{Mutex as AsyncMutex, Notify, broadcast, mpsc, watch};
use tokio::task::spawn_blocking;
use tokio::time::{sleep, timeout};
use tracing::{debug, trace, warn};

/// How long to wait for children to exit after they have been force
/// killed. A process that survives this is stuck in the kernel, and
/// waiting any longer would only hang the shutdown.
const FORCE_KILL_WAIT: Duration = Duration::from_secs(5);

pub(super) type Children = FxHashMap<u32, TrackedChild>;

/// Runs that are in progress, by cache key, for callers to wait on.
pub(super) type Inflight = scc::HashMap<String, watch::Receiver<Option<Output>>>;

/// The state shared between the registry and its background tasks. It
/// outlives a [`ProcessRegistry::stop`], so tracked children, the cache,
/// and subscribers survive a restart.
pub(super) struct RegistryState {
    pub(super) options: ProcessRegistryOptions,
    /// A synchronous lock, so that `Drop` can always reach the children.
    /// It must never be held across an await.
    pub(super) children: RwLock<Children>,
    pub(super) cache: scc::HashCache<String, Output>,
    pub(super) inflight: Inflight,
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

    /// Lock the children for reading. Every critical section is a single
    /// map operation, so the map is still consistent after a panic, and a
    /// poisoned lock is safe to recover.
    pub(super) fn read_children(&self) -> RwLockReadGuard<'_, Children> {
        self.children.read().unwrap_or_else(PoisonError::into_inner)
    }

    /// Lock the children for writing. See [`Self::read_children`].
    pub(super) fn write_children(&self) -> RwLockWriteGuard<'_, Children> {
        self.children
            .write()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Look up the child the selector refers to.
    pub(super) fn find(&self, selector: ChildSelector<'_>) -> Option<TrackedChild> {
        self.read_children()
            .get(&selector.pid())
            .filter(|tracked| selector.matches(tracked))
            .cloned()
    }

    /// Stop tracking the child the selector refers to. The match and the
    /// removal happen under one lock, so a child that reuses the pid can't
    /// slip in between and be removed instead.
    pub(super) fn release(&self, selector: ChildSelector<'_>) -> Option<TrackedChild> {
        let pid = selector.pid();

        let removed = {
            let mut children = self.write_children();

            if !children
                .get(&pid)
                .is_some_and(|tracked| selector.matches(tracked))
            {
                return None;
            }

            let removed = children.remove(&pid)?;

            self.sync_running_count(&children);
            removed
        };

        removed.child.stop_cleanup();

        trace!(pid, "Released child process");

        self.emit(ProcessEvent::Released { pid });

        Some(removed)
    }

    /// Return a snapshot of every child that is still running.
    pub(super) fn running_children(&self) -> Vec<TrackedChild> {
        self.read_children()
            .values()
            .filter(|tracked| tracked.is_running())
            .cloned()
            .collect()
    }

    /// Return true if any of these pids is tracked and still running.
    pub(super) fn any_running(&self, pids: &[u32]) -> bool {
        let children = self.read_children();

        pids.iter()
            .any(|pid| children.get(pid).is_some_and(|c| c.is_running()))
    }

    /// Publish the running count. Called with the write lock held, so a
    /// waiter woken by the change observes the new state.
    pub(super) fn sync_running_count(&self, children: &Children) {
        let count = children.values().filter(|c| c.is_running()).count();

        self.running.send_replace(count);
    }

    /// Check every running child for exit, and update those that have.
    pub(super) fn reap_exited(&self) {
        let exited = {
            let children = self.read_children();

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

        let mut children = self.write_children();

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
            self.reap_exited();

            let done = match pids {
                Some(pids) => !self.any_running(pids),
                None => !self.read_children().values().any(|c| c.is_running()),
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
        let selector = ChildSelector::Handle(&child);

        // Select by handle, as the pid may belong to a newer child by now
        let Some(tracked) = self.find(selector) else {
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

        self.release(selector);
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

        let descendants = self
            .find_descendants(vec![pid])
            .await
            .pop()
            .unwrap_or_default();

        trace!(pid, ?signal, ?descendants, "Signalling child process");

        child.send_signal(signal)?;

        kill_descendants(&descendants, signal);

        Ok(())
    }

    /// Signal many children and, when configured, their descendants. Every
    /// tree is resolved from one snapshot of the process table, taken
    /// before any child is signalled.
    pub(super) async fn signal_all(&self, children: &[TrackedChild], signal: SignalType) {
        let pids = children.iter().map(|c| c.pid()).collect();
        let trees = self.find_descendants(pids).await;

        self.send_signals(children, trees, signal);
    }

    /// Signal each child, then the descendants that were found for it.
    /// `trees` is parallel to `children`, and may be empty when descendants
    /// aren't signalled. Never blocks, so it's usable from `Drop`.
    pub(super) fn send_signals(
        &self,
        children: &[TrackedChild],
        trees: Vec<Vec<u32>>,
        signal: SignalType,
    ) {
        let mut trees = trees.into_iter();

        for tracked in children {
            let pid = tracked.pid();
            let descendants = trees.next().unwrap_or_default();

            trace!(pid, ?signal, ?descendants, "Signalling child process");

            if let Err(error) = tracked.child.send_signal(signal) {
                warn!(pid, ?signal, %error, "Failed to signal child process");
            }

            kill_descendants(&descendants, signal);
        }
    }

    /// Return the descendants of each pid, or nothing when descendants
    /// aren't signalled. Reads the process table once, off the runtime.
    async fn find_descendants(&self, pids: Vec<u32>) -> Vec<Vec<u32>> {
        if !self.options.signal_descendants {
            return vec![];
        }

        spawn_blocking(move || tree::descendants_of(&pids))
            .await
            .unwrap_or_default()
    }

    /// Shut down the running children. Returns a signal that arrived after
    /// every child of this shutdown had exited, which starts a new one.
    pub(super) async fn perform_shutdown(
        &self,
        signal: SignalType,
        mut receiver: Option<&mut broadcast::Receiver<SignalType>>,
    ) -> Option<SignalType> {
        let targets = self.running_children();

        if targets.is_empty() {
            // Readers may still be draining pipes held open by the
            // descendants of an exited child
            if matches!(signal, SignalType::Kill) {
                self.stop_all_output();
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

        self.signal_all(&targets, signal).await;

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

                    // Every child of this shutdown has already exited, so
                    // this is a new request rather than an escalation
                    if self.any_running(&pids) {
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
            let remaining = self.running_children();

            let pids = remaining.iter().map(|c| c.pid()).collect::<Vec<_>>();

            debug!(?pids, "Force killing {} child processes", remaining.len());

            self.emit(ProcessEvent::ShutdownForced { pids: pids.clone() });

            self.signal_all(&remaining, SignalType::Kill).await;
            self.stop_all_output();

            if timeout(FORCE_KILL_WAIT, self.wait_until_exited(Some(&pids)))
                .await
                .is_err()
            {
                let pids = self
                    .running_children()
                    .iter()
                    .map(|c| c.pid())
                    .filter(|pid| pids.contains(pid))
                    .collect::<Vec<_>>();

                warn!(
                    ?pids,
                    "Child processes did not exit after being force killed"
                );
            }
        }

        debug!("Shutdown of child processes finished");

        self.emit(ProcessEvent::ShutdownFinished);

        pending
    }

    /// Stop the output readers of every tracked child, running or exited,
    /// so nothing waits on a pipe inherited by a descendant.
    pub(super) fn stop_all_output(&self) {
        for tracked in self.read_children().values() {
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
