//! A registry that owns the lifetime of child processes: tracking them
//! while they run, reaping them when they exit, forwarding termination
//! signals to them (and their descendants), shutting them down as a group,
//! and caching their output. See [`ProcessRegistry`].

use crate::output::Output;
use crate::shared_child::{ChildExit, SharedChild};
use crate::signal::{SignalType, wait_for_signal};
use rustc_hash::{FxHashMap, FxHashSet};
use std::future::Future;
use std::io;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::process::{Child, Command as TokioCommand};
use tokio::runtime::Handle;
use tokio::sync::{Mutex as AsyncMutex, Notify, RwLock, broadcast, mpsc, watch};
use tokio::task::{JoinHandle, spawn_blocking};
use tokio::time::sleep;
use tracing::{debug, trace, warn};

static INSTANCE: OnceLock<Arc<ProcessRegistry>> = OnceLock::new();

/// Behavioral settings for a [`ProcessRegistry`].
#[derive(Clone, Debug)]
pub struct ProcessRegistryOptions {
    /// How long running children get to exit after a termination signal
    /// before they are force killed. A zero duration waits indefinitely.
    /// A second signal during the grace period force kills immediately.
    pub shutdown_threshold: Duration,

    /// Also deliver signals to the descendants of each child, so that a
    /// shell wrapper doesn't leave the work it spawned running. The
    /// process tree is snapshotted right before signalling, so this is
    /// best effort: a process spawned mid-signal, or one that has been
    /// re-parented, is missed.
    pub signal_descendants: bool,

    /// Listen for OS termination signals (`SIGINT`, `SIGTERM`, `SIGQUIT`
    /// on Unix; `CTRL-C`, `CTRL-BREAK`, `CTRL-CLOSE`, `CTRL-SHUTDOWN` on
    /// Windows) and shut running children down when one arrives.
    pub handle_signals: bool,

    /// Kill a child that is still running when the handle returned by
    /// [`ProcessRegistry::track`] is dropped without being released. The
    /// drop is treated as a cancellation. Clones of the handle never
    /// trigger this.
    pub kill_on_drop: bool,

    /// How often to poll tracked children for exit when no OS notification
    /// is available. On Unix, exits are detected through `SIGCHLD` and
    /// this only acts as a safety net.
    pub reap_interval: Duration,

    /// Maximum number of entries in the output cache. Least recently used
    /// entries are evicted beyond this.
    pub cache_capacity: usize,

    /// Capacity of the event and signal broadcast channels. A subscriber
    /// that falls further behind than this misses events.
    pub channel_capacity: usize,
}

impl Default for ProcessRegistryOptions {
    fn default() -> Self {
        Self {
            shutdown_threshold: Duration::from_secs(2),
            signal_descendants: true,
            handle_signals: true,
            kill_on_drop: true,
            reap_interval: Duration::from_millis(500),
            cache_capacity: 256,
            channel_capacity: 64,
        }
    }
}

/// Where a tracked child is in its lifetime.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChildState {
    /// The process has not exited yet.
    Running,

    /// The process has exited and been reaped. It stays tracked until
    /// released, so shutdown can still stop readers draining its output.
    Exited(ChildExit),
}

/// A child known to the registry.
#[derive(Clone)]
pub struct TrackedChild {
    /// The shared handle to the process.
    pub child: SharedChild,

    /// A display string of the command line, when known.
    pub command: Option<String>,

    /// Whether the process is still running.
    pub state: ChildState,

    /// When the child was tracked.
    pub tracked_at: Instant,
}

impl TrackedChild {
    /// Return the child's process id.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Return true if the process has not exited yet.
    pub fn is_running(&self) -> bool {
        self.state == ChildState::Running
    }
}

/// Something that happened in the registry. Subscribe with
/// [`ProcessRegistry::subscribe_events`].
#[derive(Clone, Debug)]
pub enum ProcessEvent {
    /// The background tasks were started.
    Started,

    /// The background tasks were stopped.
    Stopped,

    /// A child was tracked.
    Tracked { pid: u32 },

    /// A tracked child exited.
    Exited { pid: u32, exit: ChildExit },

    /// A child is no longer tracked.
    Released { pid: u32 },

    /// A termination signal was received, from the OS or programmatically.
    Signal(SignalType),

    /// Running children are being asked to shut down.
    ShutdownStarted { signal: SignalType, pids: Vec<u32> },

    /// The grace period ran out or a second signal arrived, and the
    /// children are being force killed.
    ShutdownForced { pids: Vec<u32> },

    /// Every child of the shutdown has exited.
    ShutdownFinished,
}

/// The state shared between the registry and its background tasks. It
/// outlives a [`ProcessRegistry::stop`], so tracked children, the cache,
/// and subscribers survive a restart.
struct RegistryState {
    options: ProcessRegistryOptions,
    children: RwLock<FxHashMap<u32, TrackedChild>>,
    cache: scc::HashCache<String, Output>,
    inflight: scc::HashMap<String, watch::Receiver<Option<Output>>>,
    events: broadcast::Sender<ProcessEvent>,
    signals: broadcast::Sender<SignalType>,
    running: watch::Sender<usize>,
    reap: Notify,
    cleanup_sender: mpsc::UnboundedSender<SharedChild>,
    cleanup_receiver: AsyncMutex<mpsc::UnboundedReceiver<SharedChild>>,
}

struct Tasks {
    reaper: JoinHandle<()>,
    coordinator: JoinHandle<()>,
    signals: Option<JoinHandle<()>>,
}

/// Owns the lifetime of child processes.
///
/// Children are [tracked](Self::track) (or [spawned](Self::spawn)), reaped
/// in the background when they exit, and [released](Self::release) when
/// their owner is done with them. Dropping the original handle of a child
/// that is still running kills it, so a cancelled task never leaks a
/// process. A termination signal, whether from the OS or from
/// [`Self::shutdown`], is forwarded to every running child and its
/// descendants, with a grace period before they are force killed.
///
/// The background tasks run on the tokio runtime that called
/// [`Self::start`] (or the constructor, when called inside a runtime), and
/// can be [stopped](Self::stop) and started again without losing any
/// state. A process-wide singleton is available through
/// [`Self::instance`], and independent registries through [`Self::new`].
pub struct ProcessRegistry {
    state: Arc<RegistryState>,
    tasks: Mutex<Option<Tasks>>,
}

impl Default for ProcessRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessRegistry {
    /// Create a registry with default options, and start its background
    /// tasks if called within a tokio runtime.
    pub fn new() -> Self {
        Self::with_options(ProcessRegistryOptions::default())
    }

    /// Create a registry with the given options, and start its background
    /// tasks if called within a tokio runtime. Otherwise call
    /// [`Self::start`] once a runtime is available.
    pub fn with_options(options: ProcessRegistryOptions) -> Self {
        let (events, _) = broadcast::channel(options.channel_capacity.max(1));
        let (signals, _) = broadcast::channel(options.channel_capacity.max(1));
        let (cleanup_sender, cleanup_receiver) = mpsc::unbounded_channel();

        let registry = Self {
            state: Arc::new(RegistryState {
                cache: scc::HashCache::with_capacity(0, options.cache_capacity),
                inflight: scc::HashMap::new(),
                children: RwLock::new(FxHashMap::default()),
                events,
                signals,
                running: watch::channel(0).0,
                reap: Notify::new(),
                cleanup_sender,
                cleanup_receiver: AsyncMutex::new(cleanup_receiver),
                options,
            }),
            tasks: Mutex::new(None),
        };

        registry.start();
        registry
    }

    /// Initialize the process-wide singleton with custom options. Returns
    /// false, and changes nothing, if the singleton already exists.
    pub fn register(options: ProcessRegistryOptions) -> bool {
        INSTANCE
            .set(Arc::new(ProcessRegistry::with_options(options)))
            .is_ok()
    }

    /// Return the process-wide singleton, creating it with default options
    /// if it doesn't exist yet.
    pub fn instance() -> Arc<ProcessRegistry> {
        Arc::clone(INSTANCE.get_or_init(|| Arc::new(ProcessRegistry::new())))
    }

    /// Return the process-wide singleton if it has been created.
    pub fn try_instance() -> Option<Arc<ProcessRegistry>> {
        INSTANCE.get().cloned()
    }

    /// Return the options this registry was created with.
    pub fn options(&self) -> &ProcessRegistryOptions {
        &self.state.options
    }

    // ---- Lifecycle of the registry itself ----

    /// Start the background tasks (exit reaping, signal listening, and
    /// shutdown coordination) on the current tokio runtime. Returns false
    /// if already started, or if there is no runtime.
    pub fn start(&self) -> bool {
        let Ok(handle) = Handle::try_current() else {
            return false;
        };

        let mut tasks = self.tasks.lock().unwrap();

        if tasks.is_some() {
            return false;
        }

        let state = Arc::clone(&self.state);
        let reaper = handle.spawn(run_reaper(Arc::clone(&state)));

        let receiver = state.signals.subscribe();
        let coordinator = handle.spawn(run_shutdown_coordinator(Arc::clone(&state), receiver));

        let signals = state
            .options
            .handle_signals
            .then(|| handle.spawn(wait_for_signal(state.signals.clone())));

        *tasks = Some(Tasks {
            reaper,
            coordinator,
            signals,
        });

        state.emit(ProcessEvent::Started);

        true
    }

    /// Stop the background tasks. Tracked children keep running and stay
    /// tracked, the cache is retained, and subscribers stay subscribed.
    /// Exits are no longer detected, and OS signals no longer shut children
    /// down, until [`Self::start`] is called again. Returns false if not
    /// started.
    pub fn stop(&self) -> bool {
        let Some(tasks) = self.tasks.lock().unwrap().take() else {
            return false;
        };

        tasks.reaper.abort();
        tasks.coordinator.abort();

        if let Some(signals) = tasks.signals {
            signals.abort();
        }

        self.state.emit(ProcessEvent::Stopped);

        true
    }

    /// Stop and start the background tasks, moving them onto the current
    /// tokio runtime. Returns false if they could not be started.
    pub fn restart(&self) -> bool {
        self.stop();
        self.start()
    }

    /// Return true if the background tasks are running.
    pub fn is_started(&self) -> bool {
        self.tasks.lock().unwrap().is_some()
    }

    // ---- Tracking children ----

    /// Spawn the command and track the child. The command line is
    /// recorded for [`TrackedChild::command`].
    pub async fn spawn(&self, command: &mut TokioCommand) -> io::Result<SharedChild> {
        let display = format_command(command);
        let child = command.spawn()?;

        Ok(self.track_with_command(child, Some(display)).await)
    }

    /// Wrap a spawned child in a [`SharedChild`] and track it. The returned
    /// handle is the original: dropping it while the child runs kills the
    /// child (see [`ProcessRegistryOptions::kill_on_drop`]), and dropping
    /// it after exit releases the child. Call [`Self::release`] to opt out.
    pub async fn track(&self, child: Child) -> SharedChild {
        self.track_with_command(child, None).await
    }

    /// Like [`Self::track`], recording a display string of the command line.
    pub async fn track_with_command(&self, child: Child, command: Option<String>) -> SharedChild {
        let shared = SharedChild::new_with_cleanup(child, self.state.cleanup_sender.clone());
        let pid = shared.id();

        {
            let mut children = self.state.children.write().await;

            children.insert(
                pid,
                TrackedChild {
                    child: shared.clone(),
                    command,
                    state: ChildState::Running,
                    tracked_at: Instant::now(),
                },
            );

            self.state.sync_running_count(&children);
        }

        trace!(pid, "Tracking child process");

        self.state.emit(ProcessEvent::Tracked { pid });

        // The child may have exited before we tracked it
        self.state.reap.notify_one();

        shared
    }

    /// Stop tracking a child. Does not signal it, and disables the kill
    /// on drop of its original handle. Returns the tracked entry, if any.
    pub async fn release(&self, child: &SharedChild) -> Option<TrackedChild> {
        self.release_by_pid(child.id()).await
    }

    /// Stop tracking a child by pid. See [`Self::release`].
    pub async fn release_by_pid(&self, pid: u32) -> Option<TrackedChild> {
        let removed = {
            let mut children = self.state.children.write().await;
            let removed = children.remove(&pid);

            self.state.sync_running_count(&children);
            removed
        };

        if let Some(tracked) = &removed {
            tracked.child.stop_cleanup();

            trace!(pid, "Released child process");

            self.state.emit(ProcessEvent::Released { pid });
        }

        removed
    }

    /// Look up a tracked child by pid, running or exited.
    pub async fn get(&self, pid: u32) -> Option<TrackedChild> {
        self.state.children.read().await.get(&pid).cloned()
    }

    /// Look up a child by pid, only if it is still running.
    pub async fn get_running_by_pid(&self, pid: u32) -> Option<SharedChild> {
        self.state
            .children
            .read()
            .await
            .get(&pid)
            .filter(|tracked| tracked.is_running())
            .map(|tracked| tracked.child.clone())
    }

    /// Return every tracked child, running or exited.
    pub async fn list(&self) -> Vec<TrackedChild> {
        self.state.children.read().await.values().cloned().collect()
    }

    /// Return every child that is still running.
    pub async fn list_running(&self) -> Vec<TrackedChild> {
        self.state
            .children
            .read()
            .await
            .values()
            .filter(|tracked| tracked.is_running())
            .cloned()
            .collect()
    }

    /// Return the number of children that are still running.
    pub fn running_count(&self) -> usize {
        *self.state.running.borrow()
    }

    /// Wait until no tracked child is running. Returns immediately if none
    /// are. Wrap in [`tokio::time::timeout`] to bound the wait.
    pub async fn wait_for_idle(&self) {
        self.state.wait_until_exited(None).await;
    }

    // ---- Signals ----

    /// Subscribe to termination signals received by this registry, whether
    /// from the OS or from [`Self::shutdown`].
    pub fn subscribe_signals(&self) -> broadcast::Receiver<SignalType> {
        self.state.signals.subscribe()
    }

    /// Subscribe to lifecycle events.
    pub fn subscribe_events(&self) -> broadcast::Receiver<ProcessEvent> {
        self.state.events.subscribe()
    }

    /// Send a signal to a tracked child, and its descendants when
    /// [`ProcessRegistryOptions::signal_descendants`] is set, without
    /// waiting for it to exit. Returns false if the pid is not tracked
    /// or has already exited.
    pub async fn signal(&self, pid: u32, signal: SignalType) -> io::Result<bool> {
        let Some(child) = self.get_running_by_pid(pid).await else {
            return Ok(false);
        };

        self.state.signal_tree(&child, signal).await?;

        Ok(true)
    }

    /// Send a signal to every running child, and their descendants when
    /// configured, without waiting or escalating. Use [`Self::shutdown`]
    /// for a graceful shutdown with escalation.
    pub async fn signal_running(&self, signal: SignalType) {
        for tracked in self.list_running().await {
            if let Err(error) = self.state.signal_tree(&tracked.child, signal).await {
                warn!(pid = tracked.pid(), %error, "Failed to signal child process");
            }
        }
    }

    /// Force kill a tracked child and its descendants, and wait for it to
    /// exit. Returns `None` if the pid is not tracked.
    pub async fn kill(&self, pid: u32) -> io::Result<Option<ChildExit>> {
        let Some(tracked) = self.get(pid).await else {
            return Ok(None);
        };

        self.state
            .signal_tree(&tracked.child, SignalType::Kill)
            .await?;

        Ok(Some(tracked.child.wait().await?))
    }

    /// Shut down every running child: signal them (and their descendants),
    /// give them [`ProcessRegistryOptions::shutdown_threshold`] to exit, then
    /// force kill whatever remains, and return once they have all exited.
    /// Children stay tracked until released. When the background tasks are
    /// running, this is the same path an OS signal takes; otherwise the
    /// shutdown runs inline.
    pub async fn shutdown(&self, signal: SignalType) {
        if self.is_started() {
            let mut events = self.state.events.subscribe();

            // The coordinator subscribes when started, so a send only fails
            // if it has died, in which case run the shutdown ourselves.
            if self.state.signals.send(signal).is_ok() {
                loop {
                    match events.recv().await {
                        Ok(ProcessEvent::ShutdownFinished)
                        | Err(broadcast::error::RecvError::Closed) => {
                            return;
                        }
                        Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                    }
                }
            }
        }

        self.state.emit(ProcessEvent::Signal(signal));
        self.state.perform_shutdown(signal, None).await;
    }

    /// Broadcast a termination signal to the registry without waiting for
    /// the shutdown to complete. Requires the background tasks to be
    /// running; see [`Self::shutdown`] otherwise.
    pub fn terminate_running(&self) {
        let _ = self.state.signals.send(SignalType::Terminate);
    }

    // ---- Output cache ----

    /// Return the cached output for this key.
    pub async fn get_cached_output(&self, key: &str) -> Option<Output> {
        self.state
            .cache
            .read_async(key, |_, output| output.clone())
            .await
    }

    /// Cache the output for this key. An existing entry is kept, as an
    /// identical command produced it.
    pub async fn cache_output(&self, key: impl Into<String>, output: Output) {
        let _ = self.state.cache.put_async(key.into(), output).await;
    }

    /// Remove the cached output for this key.
    pub async fn remove_cached_output(&self, key: &str) -> Option<Output> {
        self.state
            .cache
            .remove_async(key)
            .await
            .map(|(_, output)| output)
    }

    /// Remove every cached output.
    pub async fn clear_cache(&self) {
        self.state.cache.clear_async().await;
    }

    /// Return the cached output for this key, or run `exec` to produce and
    /// cache it. Concurrent calls with the same key share a single run: the
    /// first caller executes while the others wait for its output. A failed
    /// run is not cached, and the waiters then run `exec` themselves.
    pub async fn exec_cached<F, E>(&self, key: impl Into<String>, exec: F) -> Result<Output, E>
    where
        F: Future<Output = Result<Output, E>>,
    {
        let key = key.into();

        if let Some(output) = self.get_cached_output(&key).await {
            return Ok(output);
        }

        let sender = match self.state.inflight.entry_async(key.clone()).await {
            scc::hash_map::Entry::Occupied(entry) => {
                let mut receiver = entry.get().clone();
                drop(entry);

                match receiver.wait_for(|output| output.is_some()).await {
                    Ok(output) => return Ok(output.clone().unwrap()),
                    // The run failed, so try it ourselves
                    Err(_) => None,
                }
            }
            scc::hash_map::Entry::Vacant(entry) => {
                let (sender, receiver) = watch::channel(None);
                entry.insert_entry(receiver);
                Some(sender)
            }
        };

        let result = exec.await;

        if let Ok(output) = &result {
            self.cache_output(key.clone(), output.clone()).await;

            if let Some(sender) = &sender {
                let _ = sender.send(Some(output.clone()));
            }
        }

        // Only the runner owns the in-flight entry. A failed run drops the
        // sender without a value, which tells the waiters to run themselves.
        if sender.is_some() {
            self.state.inflight.remove_async(&key).await;
        }

        result
    }
}

impl Drop for ProcessRegistry {
    fn drop(&mut self) {
        self.stop();

        // Nothing can await here, so signal synchronously. Children that
        // are already dead are skipped, and readers of every tracked child
        // are stopped so nothing waits on a pipe held by a descendant.
        if let Ok(children) = self.state.children.try_read() {
            for tracked in children.values() {
                let pid = tracked.pid();

                if tracked.is_running() {
                    let descendants = if self.state.options.signal_descendants {
                        tree::descendants(pid)
                    } else {
                        vec![]
                    };

                    if let Err(error) = tracked.child.send_signal(SignalType::Kill) {
                        warn!(pid, %error, "Failed to kill child process while dropping registry");
                    }

                    kill_descendants(&descendants, SignalType::Kill);
                }

                tracked.child.stop_output();
            }
        }
    }
}

impl RegistryState {
    fn emit(&self, event: ProcessEvent) {
        let _ = self.events.send(event);
    }

    /// Publish the running count. Called with the write lock held, so a
    /// waiter woken by the change observes the new state.
    fn sync_running_count(&self, children: &FxHashMap<u32, TrackedChild>) {
        let count = children.values().filter(|c| c.is_running()).count();

        self.running.send_replace(count);
    }

    /// Check every running child for exit, and update those that have.
    async fn reap_exited(&self) {
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
    async fn wait_until_exited(&self, pids: Option<&[u32]>) {
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
    async fn cleanup_dropped(self: Arc<Self>, child: SharedChild) {
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
    async fn signal_tree(&self, child: &SharedChild, signal: SignalType) -> io::Result<()> {
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
    async fn perform_shutdown(
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

/// Detect exited children and release dropped ones.
async fn run_reaper(state: Arc<RegistryState>) {
    // Holding the lock for the life of the task hands the receiver back
    // when the task is aborted, so a restart resumes where it left off
    let mut cleanup = state.cleanup_receiver.lock().await;

    #[cfg(unix)]
    let mut sigchld = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::child()).ok();

    #[cfg(not(unix))]
    let mut sigchld = ();

    let mut interval = tokio::time::interval(state.options.reap_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    // Children may have exited while we weren't running
    state.reap_exited().await;

    loop {
        tokio::select! {
            Some(child) = cleanup.recv() => {
                // A kill waits for the child to exit, so don't block the
                // reaper on it
                tokio::spawn(Arc::clone(&state).cleanup_dropped(child));
            },
            _ = state.reap.notified() => state.reap_exited().await,
            _ = interval.tick() => state.reap_exited().await,
            _ = wait_for_sigchld(&mut sigchld) => state.reap_exited().await,
        }
    }
}

#[cfg(unix)]
async fn wait_for_sigchld(signal: &mut Option<tokio::signal::unix::Signal>) {
    match signal {
        Some(signal) => {
            signal.recv().await;
        }
        None => std::future::pending().await,
    }
}

#[cfg(not(unix))]
async fn wait_for_sigchld(_signal: &mut ()) {
    std::future::pending().await
}

/// Turn every received signal into a shutdown of the running children.
async fn run_shutdown_coordinator(
    state: Arc<RegistryState>,
    mut receiver: broadcast::Receiver<SignalType>,
) {
    let mut pending = None;

    loop {
        let signal = match pending.take() {
            Some(signal) => signal,
            None => match receiver.recv().await {
                Ok(signal) => signal,
                Err(broadcast::error::RecvError::Lagged(_)) => SignalType::Kill,
                Err(broadcast::error::RecvError::Closed) => break,
            },
        };

        state.emit(ProcessEvent::Signal(signal));

        pending = state.perform_shutdown(signal, Some(&mut receiver)).await;
    }
}

fn kill_descendants(pids: &[u32], signal: SignalType) {
    for pid in pids {
        if let Err(error) = tree::kill(*pid, signal) {
            debug!(pid, %error, "Failed to signal descendant process");
        }
    }
}

fn format_command(command: &TokioCommand) -> String {
    let command = command.as_std();
    let mut line = command.get_program().to_string_lossy().into_owned();

    for arg in command.get_args() {
        line.push(' ');
        line.push_str(&arg.to_string_lossy());
    }

    line
}

/// Best effort enumeration and signalling of process trees.
mod tree {
    use super::*;

    /// Return every descendant of `pid` (children, grandchildren, and so
    /// on), parents before their children. Failures yield an empty list.
    pub fn descendants(pid: u32) -> Vec<u32> {
        let mut children_of = imp::children_lookup();
        let mut seen = FxHashSet::default();
        let mut queue = vec![pid];
        let mut found = vec![];

        seen.insert(pid);

        while let Some(parent) = queue.pop() {
            for child in children_of(parent) {
                if seen.insert(child) {
                    found.push(child);
                    queue.push(child);
                }
            }
        }

        found
    }

    /// Signal a process that isn't one of our children by pid.
    pub fn kill(pid: u32, signal: SignalType) -> io::Result<()> {
        imp::kill(pid, signal)
    }

    /// Build a parent -> children lookup from a `(pid, ppid)` list.
    #[cfg(not(target_os = "macos"))]
    fn lookup_from_pairs(pairs: Vec<(u32, u32)>) -> impl FnMut(u32) -> Vec<u32> {
        let mut map: FxHashMap<u32, Vec<u32>> = FxHashMap::default();

        for (pid, ppid) in pairs {
            map.entry(ppid).or_default().push(pid);
        }

        move |ppid| map.remove(&ppid).unwrap_or_default()
    }

    #[cfg(target_os = "linux")]
    mod imp {
        use super::*;

        pub fn children_lookup() -> impl FnMut(u32) -> Vec<u32> {
            let mut pairs = vec![];

            if let Ok(dir) = std::fs::read_dir("/proc") {
                for entry in dir.flatten() {
                    let Some(pid) = entry
                        .file_name()
                        .to_str()
                        .and_then(|name| name.parse::<u32>().ok())
                    else {
                        continue;
                    };

                    let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
                        continue;
                    };

                    // `pid (comm) state ppid ...`, where comm may contain
                    // spaces and parentheses, so split on the last `)`
                    let Some(ppid) = stat
                        .rsplit_once(')')
                        .and_then(|(_, rest)| rest.split_whitespace().nth(1))
                        .and_then(|ppid| ppid.parse::<u32>().ok())
                    else {
                        continue;
                    };

                    pairs.push((pid, ppid));
                }
            }

            lookup_from_pairs(pairs)
        }

        pub fn kill(pid: u32, signal: SignalType) -> io::Result<()> {
            crate::signal::kill(pid, signal)
        }
    }

    #[cfg(target_os = "macos")]
    mod imp {
        use super::*;
        use std::ffi::c_int;
        use std::mem::size_of;

        pub fn children_lookup() -> impl FnMut(u32) -> Vec<u32> {
            |ppid| {
                let mut capacity = 64;

                loop {
                    let mut buffer = vec![0 as libc::pid_t; capacity];
                    let size = (capacity * size_of::<libc::pid_t>()) as c_int;

                    // Returns the number of pids written
                    let written = unsafe {
                        libc::proc_listchildpids(
                            ppid as libc::pid_t,
                            buffer.as_mut_ptr().cast(),
                            size,
                        )
                    };

                    if written <= 0 {
                        return vec![];
                    }

                    let count = written as usize;

                    // A full buffer may have been truncated
                    if count >= capacity && capacity < (1 << 16) {
                        capacity *= 2;
                        continue;
                    }

                    buffer.truncate(count);

                    return buffer
                        .into_iter()
                        .filter(|pid| *pid > 0)
                        .map(|pid| pid as u32)
                        .collect();
                }
            }
        }

        pub fn kill(pid: u32, signal: SignalType) -> io::Result<()> {
            crate::signal::kill(pid, signal)
        }
    }

    #[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
    mod imp {
        use super::*;

        pub fn children_lookup() -> impl FnMut(u32) -> Vec<u32> {
            let output = std::process::Command::new("ps")
                .args(["-A", "-o", "pid=,ppid="])
                .output();

            let pairs = match output {
                Ok(output) => String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .filter_map(|line| {
                        let mut parts = line.split_whitespace();
                        let pid = parts.next()?.parse().ok()?;
                        let ppid = parts.next()?.parse().ok()?;
                        Some((pid, ppid))
                    })
                    .collect(),
                Err(_) => vec![],
            };

            lookup_from_pairs(pairs)
        }

        pub fn kill(pid: u32, signal: SignalType) -> io::Result<()> {
            crate::signal::kill(pid, signal)
        }
    }

    #[cfg(windows)]
    mod imp {
        use super::*;
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32, Process32First, Process32Next,
            TH32CS_SNAPPROCESS,
        };
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_TERMINATE, TerminateProcess,
        };

        const ERROR_ACCESS_DENIED: i32 = 5;
        const ERROR_INVALID_PARAMETER: i32 = 87;

        pub fn children_lookup() -> impl FnMut(u32) -> Vec<u32> {
            let mut pairs = vec![];
            let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };

            if snapshot != INVALID_HANDLE_VALUE {
                let mut entry: PROCESSENTRY32 = unsafe { std::mem::zeroed() };
                entry.dwSize = std::mem::size_of::<PROCESSENTRY32>() as u32;

                if unsafe { Process32First(snapshot, &mut entry) } != 0 {
                    loop {
                        pairs.push((entry.th32ProcessID, entry.th32ParentProcessID));

                        if unsafe { Process32Next(snapshot, &mut entry) } == 0 {
                            break;
                        }
                    }
                }

                unsafe { CloseHandle(snapshot) };
            }

            lookup_from_pairs(pairs)
        }

        /// `Interrupt` is a no-op, as `CTRL-C` can't be targeted at a
        /// process; everything else terminates it. A process that no longer
        /// exists is treated as already dead.
        pub fn kill(pid: u32, signal: SignalType) -> io::Result<()> {
            if matches!(signal, SignalType::Interrupt) {
                return Ok(());
            }

            let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };

            if handle.is_null() {
                let error = io::Error::last_os_error();

                return if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER) {
                    Ok(())
                } else {
                    Err(error)
                };
            }

            let result = unsafe { TerminateProcess(handle, 1) };
            let error = io::Error::last_os_error();

            unsafe { CloseHandle(handle) };

            if result == 0 && error.raw_os_error() != Some(ERROR_ACCESS_DENIED) {
                return Err(error);
            }

            Ok(())
        }
    }
}
