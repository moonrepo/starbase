use crate::output::Output;
use crate::shared_child::*;
use crate::signal::*;
use core::time::Duration;
use rustc_hash::FxHashMap;
use std::sync::{Arc, OnceLock};
use tokio::process::Child;
use tokio::sync::RwLock;
use tokio::sync::broadcast::{self, Receiver, Sender};
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::sleep;
use tracing::{debug, warn};

static INSTANCE: OnceLock<Arc<ProcessRegistry>> = OnceLock::new();

pub type RunningProcessesMap = Arc<RwLock<FxHashMap<u32, SharedChild>>>;

/// Tracks running child processes so they can be looked up, shut down as
/// a group on a termination signal, and have their output cached across
/// runs. A process-wide singleton is available via [`Self::instance`].
pub struct ProcessRegistry {
    /// The output cache, keyed by [`Command::get_cache_key`](crate::Command::get_cache_key).
    pub cache: Arc<scc::HashCache<String, Output>>,

    /// Milliseconds to wait for running processes to shut down gracefully
    /// after a termination signal, before force killing them. `0` waits
    /// on them indefinitely instead of force killing. A second signal
    /// skips this wait while any child from the first shutdown is running.
    pub threshold: u32,

    running: RunningProcessesMap,
    cleanup_sender: UnboundedSender<SharedChild>,
    cleanup_handle: JoinHandle<()>,
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
        let processes_background = Arc::clone(&processes);
        let processes_cleanup = Arc::clone(&processes);

        let (sender, receiver) = broadcast::channel::<SignalType>(10);
        let sender_bg = sender.clone();
        let (cleanup_sender, cleanup_receiver) = mpsc::unbounded_channel();

        let cleanup_handle = tokio::spawn(async move {
            cleanup_cancelled_processes(cleanup_receiver, processes_cleanup).await;
        });

        let signal_wait_handle = tokio::spawn(async move {
            wait_for_signal(sender_bg).await;
        });

        let signal_shutdown_handle = tokio::spawn(async move {
            shutdown_processes_from_signal(receiver, processes_background, threshold).await;
        });

        Self {
            cache: Arc::new(scc::HashCache::new()),
            running: processes,
            cleanup_sender,
            signal_sender: sender,
            cleanup_handle,
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

    pub(crate) async fn get_cached_output(&self, key: &str) -> miette::Result<Option<Output>> {
        Ok(self.cache.read_async(key, |_, output| output.clone()).await)
    }

    pub(crate) async fn cache_output(&self, key: String, output: Output) {
        // Another identical command may have completed while this one ran.
        // Keeping the first result is sufficient because both have the same
        // cache identity.
        let _ = self.cache.put_async(key, output).await;
    }

    /// Wrap a spawned child and register it as running, so it's tracked
    /// for lookup and shutdown.
    pub async fn add_running(&self, child: Child) -> SharedChild {
        let shared = SharedChild::new_with_cleanup(child, self.cleanup_sender.clone());

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
        if let Some(child) = self.running.write().await.remove(&id) {
            child.stop_cleanup();
        }
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
        // Drop cannot await the shutdown task. Signal the children directly
        // before aborting the listeners so they cannot survive registry
        // destruction merely because the broadcast was not consumed.
        if let Ok(children) = self.running.try_read() {
            for child in children.values() {
                if let Err(error) = child.send_signal(SignalType::Kill) {
                    warn!(
                        pid = child.id(),
                        %error,
                        "Failed to kill child process while dropping registry",
                    );
                }
            }
        }

        self.cleanup_handle.abort();
        self.signal_wait_handle.abort();
        self.signal_shutdown_handle.abort();
    }
}

async fn cleanup_cancelled_processes(
    mut receiver: mpsc::UnboundedReceiver<SharedChild>,
    processes: RunningProcessesMap,
) {
    while let Some(child) = receiver.recv().await {
        cleanup_cancelled_process(child, Arc::clone(&processes)).await;
    }
}

async fn cleanup_cancelled_process(child: SharedChild, processes: RunningProcessesMap) {
    let pid = child.id();

    if let Err(error) = child.kill_with_signal(SignalType::Kill).await {
        warn!(pid, %error, "Failed to clean up cancelled child process");
    }

    processes.write().await.remove(&pid);
}

async fn shutdown_processes_from_signal(
    mut receiver: Receiver<SignalType>,
    processes: RunningProcessesMap,
    threshold: u32,
) {
    let mut pending_signal = None;

    loop {
        let signal = match pending_signal.take() {
            Some(signal) => signal,
            None => match receiver.recv().await {
                Ok(signal) => signal,
                Err(broadcast::error::RecvError::Lagged(_)) => SignalType::Kill,
                Err(broadcast::error::RecvError::Closed) => break,
            },
        };

        pending_signal =
            shutdown_processes(signal, processes.clone(), &mut receiver, threshold).await;
    }
}

async fn receive_repeated_signal(receiver: &mut Receiver<SignalType>) -> SignalType {
    match receiver.recv().await {
        Ok(signal) => signal,
        Err(_) => SignalType::Kill,
    }
}

async fn shutdown_processes(
    signal: SignalType,
    processes: RunningProcessesMap,
    receiver: &mut Receiver<SignalType>,
    threshold: u32,
) -> Option<SignalType> {
    // Clone the children, otherwise we encounter a deadlock when the
    // tasks try to acquire a write lock while it is being read
    let children = { processes.read().await.clone() };

    if children.is_empty() {
        return None;
    }

    // Attempt to gracefully shutdown running processes
    debug!(
        signal = ?signal,
        pids = ?children.keys().collect::<Vec<_>>(),
        "Shutting down {} running child processes",
        children.len()
    );

    let mut set = JoinSet::new();

    // Retain the original handles through escalation: the direct child may
    // exit and be unregistered while descendants still hold its output pipes.
    let force_children = children.clone();

    for (pid, child) in children {
        let running = processes.clone();

        set.spawn(async move {
            if threshold == 0 {
                debug!(pid, "Signalling and waiting on child process");

                if let Err(error) = child.kill_with_signal(signal).await {
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

    let repeated_signal = if matches!(signal, SignalType::Kill) {
        Some(signal)
    } else if threshold == 0 {
        Some(receive_repeated_signal(receiver).await)
    } else {
        tokio::select! {
            _ = sleep(Duration::from_millis(threshold as u64)) => None,
            signal = receive_repeated_signal(receiver) => Some(signal),
        }
    };

    let mut force_children = force_children;

    if let Some(repeated_signal) = repeated_signal {
        let running = processes.read().await.clone();

        // Once every child from the first shutdown has exited, this is
        // a new shutdown request rather than an escalation of the old one.
        if !force_children.keys().any(|pid| running.contains_key(pid)) {
            for child in force_children.values() {
                child.stop_output();
            }

            return Some(repeated_signal);
        }

        debug!(
            signal = ?repeated_signal,
            "Received another signal during shutdown; force killing children"
        );

        // The repeated signal applies to the entire registry, including
        // children that were registered after shutdown began.
        force_children.extend(running);
    }

    kill_processes(processes, force_children.clone()).await;

    for child in force_children.values() {
        child.stop_output();
    }

    // Wait for things to finish
    set.join_all().await;

    None
}

async fn kill_processes(processes: RunningProcessesMap, children: FxHashMap<u32, SharedChild>) {
    if children.is_empty() {
        return;
    }

    let child_pids = children.keys().copied().collect::<Vec<_>>();

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

    // Only remove the children that belonged to this shutdown request. A
    // later signal may have registered new children while this escalation
    // timer was running.
    let mut running = processes.write().await;

    for pid in child_pids {
        running.remove(&pid);
    }
}
