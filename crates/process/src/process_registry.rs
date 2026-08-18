use crate::output::Output;
use crate::shared_child::*;
use crate::signal::*;
use core::time::Duration;
use rustc_hash::FxHashMap;
use std::sync::{Arc, OnceLock};
use tokio::process::Child;
use tokio::sync::RwLock;
use tokio::sync::broadcast::{self, Receiver, Sender};
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::sleep;
use tracing::{debug, warn};

static INSTANCE: OnceLock<Arc<ProcessRegistry>> = OnceLock::new();

/// Tracks running child processes so they can be looked up, shut down as
/// a group on a termination signal, and have their output cached across
/// runs. A process-wide singleton is available via [`Self::instance`].
pub struct ProcessRegistry {
    /// The output cache, keyed by [`Command::get_cache_key`](crate::Command::get_cache_key).
    pub cache: Arc<scc::HashCache<String, Output>>,

    /// Milliseconds to wait for running processes to shut down gracefully
    /// after a termination signal, before force killing them. `0` waits
    /// on them indefinitely instead of force killing.
    pub threshold: u32,

    running: Arc<RwLock<FxHashMap<u32, SharedChild>>>,
    signal_sender: Sender<SignalType>,
    signal_wait_handle: JoinHandle<()>,
    signal_shutdown_handle: JoinHandle<()>,
}

impl Default for ProcessRegistry {
    fn default() -> Self {
        Self::new(2000) // 2 seconds
    }
}

impl ProcessRegistry {
    /// Create a new registry, listening for termination signals in the
    /// background. `threshold` is the grace period, in milliseconds,
    /// given to running processes to shut down before they're force
    /// killed; `0` waits on them indefinitely instead.
    pub fn new(threshold: u32) -> Self {
        let processes = Arc::new(RwLock::new(FxHashMap::default()));
        let processes_bg = Arc::clone(&processes);

        let (sender, receiver) = broadcast::channel::<SignalType>(10);
        let sender_bg = sender.clone();

        let signal_wait_handle = tokio::spawn(async move {
            wait_for_signal(sender_bg).await;
        });

        let signal_shutdown_handle = tokio::spawn(async move {
            shutdown_processes_from_signal(receiver, processes_bg, threshold).await;
        });

        Self {
            cache: Arc::new(scc::HashCache::new()),
            running: processes,
            signal_sender: sender,
            signal_wait_handle,
            signal_shutdown_handle,
            threshold,
        }
    }

    /// Initialize the process-wide singleton returned by [`Self::instance`]
    /// with a custom shutdown threshold. Has no effect if the singleton
    /// has already been initialized, whether by this or a prior call to
    /// [`Self::instance`].
    pub fn register(threshold: u32) {
        let _ = INSTANCE.set(Arc::new(ProcessRegistry::new(threshold)));
    }

    /// Return the process-wide singleton, creating it with the default
    /// threshold if it doesn't exist yet.
    pub fn instance() -> Arc<ProcessRegistry> {
        Arc::clone(INSTANCE.get_or_init(|| Arc::new(ProcessRegistry::default())))
    }

    /// Wrap a spawned child and register it as running, so it's tracked
    /// for lookup and shutdown.
    pub async fn add_running(&self, child: Child) -> SharedChild {
        let shared = SharedChild::new(child);

        self.running
            .write()
            .await
            .insert(shared.id(), shared.clone());

        shared
    }

    /// Look up a running child by pid.
    pub async fn get_running_by_pid(&self, id: u32) -> Option<SharedChild> {
        self.running.read().await.get(&id).cloned()
    }

    /// Stop tracking a child as running. Does not kill or signal it.
    pub async fn remove_running(&self, child: SharedChild) {
        self.remove_running_by_pid(child.id()).await
    }

    /// Stop tracking a child by pid as running. Does not kill or signal it.
    pub async fn remove_running_by_pid(&self, id: u32) {
        self.running.write().await.remove(&id);
    }

    /// Subscribe to termination signals received by this registry,
    /// whether from the OS or from [`Self::terminate_running`].
    pub fn receive_signal(&self) -> Receiver<SignalType> {
        self.signal_sender.subscribe()
    }

    /// Broadcast a termination signal to all running children, shutting
    /// them down the same way an OS-delivered signal would.
    pub fn terminate_running(&self) {
        let _ = self.signal_sender.send(SignalType::Terminate);
    }

    /// Wait until no processes are tracked as running, polling every 50
    /// milliseconds. Returns immediately if none are running, and after
    /// [`Self::threshold`] milliseconds regardless, unless it's `0`.
    pub async fn wait_for_running_to_shutdown(&self) {
        let mut count = 0;

        loop {
            // After many seconds of waiting, just exit immediately
            if self.threshold > 0 && count >= self.threshold {
                break;
            }

            // Wait for all running processes to have stopped
            if self.running.read().await.is_empty() {
                break;
            }

            sleep(Duration::from_millis(50)).await;
            count += 50;
        }
    }
}

impl Drop for ProcessRegistry {
    fn drop(&mut self) {
        self.terminate_running();
        self.signal_wait_handle.abort();
        self.signal_shutdown_handle.abort();
    }
}

async fn shutdown_processes_from_signal(
    mut receiver: Receiver<SignalType>,
    processes: Arc<RwLock<FxHashMap<u32, SharedChild>>>,
    threshold: u32,
) {
    let signal = receiver.recv().await.unwrap_or(SignalType::Kill);

    // Clone the children, otherwise we encounter a deadlock when the
    // tasks try to acquire a write lock while it is being read
    let children = { processes.read().await.clone() };

    if children.is_empty() {
        return;
    }

    // Attempt to gracefully shutdown running processes
    debug!(
        signal = ?signal,
        pids = ?children.keys().collect::<Vec<_>>(),
        "Shutting down {} running child processes",
        children.len()
    );

    let mut set = JoinSet::new();

    for (pid, child) in children {
        let running = processes.clone();

        set.spawn(async move {
            if threshold == 0 {
                debug!(pid, "Waiting on child process");

                if let Err(error) = child.wait().await {
                    warn!(
                        pid,
                        error = error.to_string(),
                        "Failed to wait on child process"
                    );
                }
            } else {
                debug!(pid, "Shutting down child process");

                if let Err(error) = child.kill_with_signal(signal).await {
                    warn!(
                        pid,
                        error = error.to_string(),
                        "Failed to shutdown child process"
                    );
                }
            }

            running.write().await.remove(&pid);
        });
    }

    // Otherwise force kill running processes after the threshold
    let running = processes.clone();

    set.spawn(async move {
        if threshold > 0 {
            sleep(Duration::from_millis(threshold as u64)).await;

            kill_processes(running).await
        }
    });

    // Wait for things to finish
    set.join_all().await;
}

async fn kill_processes(processes: Arc<RwLock<FxHashMap<u32, SharedChild>>>) {
    let children = { processes.read().await.clone() };

    if children.is_empty() {
        return;
    }

    debug!(
        pids = ?children.keys().collect::<Vec<_>>(),
        "Wait threshold exhausted, killing {} running child processes",
        children.len()
    );

    let mut set = JoinSet::new();

    for (pid, child) in children {
        set.spawn(async move {
            debug!(pid, "Killing child process");

            if let Err(error) = child.kill_with_signal(SignalType::Kill).await {
                warn!(
                    pid,
                    error = error.to_string(),
                    "Failed to kill child process"
                );
            }
        });
    }

    set.join_all().await;
    processes.write().await.clear();
}
