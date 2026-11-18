use crate::output::Output;
use crate::output_cache::OutputCache;
use crate::process_set::ProcessSet;
use crate::shared_child::*;
use crate::signal::*;
use crate::signal_listener::{self, SignalListener};
use core::time::Duration;
use rustc_hash::FxHashMap;
use std::sync::{Arc, OnceLock};
use tokio::process::Child;
use tokio::runtime::Handle;
use tokio::sync::RwLock;
use tokio::sync::broadcast::{self, Receiver, Sender};
use tokio::task::JoinHandle;
use tokio::time::timeout;

static INSTANCE: OnceLock<Arc<ProcessRegistry>> = OnceLock::new();

pub type RunningProcessesMap = Arc<RwLock<FxHashMap<u32, SharedChild>>>;

/// Tracks running child processes so they can be looked up, shut down as
/// a group on a termination signal, and have their output cached across
/// runs. A process-wide singleton is available via [`Self::instance`],
/// which is a layer over [`ProcessSet::root`]. Every other registry is a
/// layer over a set nested under the root.
pub struct ProcessRegistry {
    /// The output cache, keyed by [`Command::get_cache_key`](crate::Command::get_cache_key).
    /// Every registry shares the one process-wide cache.
    pub cache: Arc<scc::HashCache<String, Output>>,

    /// Milliseconds to wait for running processes to shut down gracefully
    /// after a termination signal, before force killing them. `0` waits
    /// on them indefinitely instead of force killing. A second signal
    /// skips this wait while any child from the first shutdown is running.
    pub threshold: u32,

    set: ProcessSet,
    owns_set: bool,
    signal_sender: Sender<SignalType>,
    signal_forward_handle: JoinHandle<()>,
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
    /// killed; `0` waits on them indefinitely instead. Its children are
    /// tracked in a set nested under [`ProcessSet::root`].
    pub fn new(threshold: u32) -> Self {
        let set = ProcessSet::root().nested_with_grace(Duration::from_millis(threshold.into()));

        Self::with_set(set, threshold, true)
    }

    fn with_set(set: ProcessSet, threshold: u32, owns_set: bool) -> Self {
        let (sender, _) = broadcast::channel::<SignalType>(10);
        let sender_bg = sender.clone();

        // Subscribe before spawning, so no signal is missed in between
        let os_signals = SignalListener::subscribe();

        let signal_forward_handle = tokio::spawn(async move {
            forward_signals(os_signals, sender_bg).await;
        });

        Self {
            cache: Arc::clone(&OutputCache::instance().entries),
            threshold,
            set,
            owns_set,
            signal_sender: sender,
            signal_forward_handle,
        }
    }

    /// Initialize the process-wide singleton returned by [`Self::instance`]
    /// with a custom shutdown threshold, which also becomes the grace
    /// period of [`ProcessSet::root`]. Has no effect if the singleton has
    /// already been initialized, whether by this or a prior call to
    /// [`Self::instance`].
    pub fn register(threshold: u32) {
        INSTANCE.get_or_init(|| {
            ProcessSet::configure_root(Duration::from_millis(threshold.into()));

            Arc::new(Self::with_set(ProcessSet::root(), threshold, false))
        });
    }

    /// Return the process-wide singleton, creating it with the root's
    /// grace period as its threshold if it doesn't exist yet.
    pub fn instance() -> Arc<ProcessRegistry> {
        Arc::clone(INSTANCE.get_or_init(|| {
            let root = ProcessSet::root();
            let threshold = u32::try_from(root.grace().as_millis()).unwrap_or(u32::MAX);

            Arc::new(Self::with_set(root, threshold, false))
        }))
    }

    /// Wrap a spawned child and register it as running, so it's tracked
    /// for lookup and shutdown.
    pub async fn add_running(&self, child: Child) -> SharedChild {
        self.set.track(child)
    }

    /// Look up a running child by pid.
    pub async fn get_running_by_pid(&self, id: u32) -> Option<SharedChild> {
        self.set.get(id)
    }

    /// Stop tracking a child as running. Does not kill or signal it.
    pub async fn remove_running(&self, child: SharedChild) {
        self.remove_running_by_pid(child.id()).await
    }

    /// Stop tracking a child by pid as running. Does not kill or signal it.
    pub async fn remove_running_by_pid(&self, id: u32) {
        self.set.release(id);
    }

    /// Subscribe to termination signals received by this registry,
    /// whether from the OS or from [`Self::terminate_running`].
    pub fn receive_signal(&self) -> Receiver<SignalType> {
        self.signal_sender.subscribe()
    }

    /// Broadcast a termination signal to subscribers, and shut down the
    /// running children the same way an OS-delivered signal would. For the
    /// singleton, that includes every set nested under the root.
    pub fn terminate_running(&self) {
        let _ = self.signal_sender.send(SignalType::Terminate);

        let set = self.set.clone();
        let shutdown = async move { set.shutdown(SignalType::Terminate).await };

        // Run on the caller's runtime, as the shutdown task this replaces did
        match Handle::try_current() {
            Ok(handle) => {
                handle.spawn(shutdown);
            }
            Err(_) => signal_listener::spawn(shutdown),
        }
    }

    /// Wait until no processes are tracked as running. Returns immediately
    /// if none are running, and after [`Self::threshold`] milliseconds
    /// regardless, unless it's `0`.
    pub async fn wait_for_running_to_shutdown(&self) {
        if self.threshold > 0 {
            let _ = timeout(
                Duration::from_millis(self.threshold.into()),
                self.set.wait_idle(),
            )
            .await;
        } else {
            self.set.wait_idle().await;
        }
    }
}

impl Drop for ProcessRegistry {
    fn drop(&mut self) {
        // Drop cannot await a shutdown, so kill the children directly. The
        // root set outlives every registry, so it's left alone.
        if self.owns_set {
            self.set.kill_all();
        }

        self.signal_forward_handle.abort();
    }
}

/// Pass each OS signal from the [`SignalListener`] on to this registry's
/// subscribers. Runs until the registry is dropped.
async fn forward_signals(mut receiver: Receiver<SignalType>, sender: Sender<SignalType>) {
    loop {
        match receiver.recv().await {
            Ok(signal) => {
                // Having no subscribers right now is fine
                let _ = sender.send(signal);
            }
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}
