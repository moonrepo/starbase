use super::child::{ChildSelector, ChildState, TrackedChild};
use super::event::ProcessEvent;
use super::options::ProcessRegistryOptions;
use super::state::RegistryState;
use super::tasks::{Tasks, run_reaper, run_shutdown_coordinator};
use super::tree;
use crate::shared_child::{ChildExit, SharedChild};
use crate::signal::{SignalType, wait_for_signal};
use rustc_hash::FxHashMap;
use std::io;
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Instant;
use tokio::process::{Child, Command as TokioCommand};
use tokio::runtime::Handle;
use tokio::sync::{Mutex as AsyncMutex, Notify, broadcast, mpsc, watch};
use tracing::trace;

static INSTANCE: OnceLock<Arc<ProcessRegistry>> = OnceLock::new();

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
    pub(super) state: Arc<RegistryState>,
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

        let replaced = {
            let mut children = self.state.write_children();

            let replaced = children.insert(
                pid,
                TrackedChild {
                    child: shared.clone(),
                    command,
                    state: ChildState::Running,
                    tracked_at: Instant::now(),
                },
            );

            self.state.sync_running_count(&children);
            replaced
        };

        // The OS gave this child the pid of one that has exited but was
        // still tracked. Its handle no longer selects anything.
        if replaced.is_some() {
            trace!(pid, "Released stale child process with a reused pid");

            self.state.emit(ProcessEvent::Released { pid });
        }

        trace!(pid, "Tracking child process");

        self.state.emit(ProcessEvent::Tracked { pid });

        // The child may have exited before we tracked it
        self.state.reap.notify_one();

        shared
    }

    /// Stop tracking a child, selected by pid or by handle (see
    /// [`ChildSelector`]). Does not signal it, and disables the kill on
    /// drop of its original handle. Returns the tracked entry, if any.
    pub async fn release<'a>(&self, child: impl Into<ChildSelector<'a>>) -> Option<TrackedChild> {
        self.state.release(child.into())
    }

    /// Look up a tracked child by pid or by handle, running or exited.
    pub async fn get<'a>(&self, child: impl Into<ChildSelector<'a>>) -> Option<TrackedChild> {
        self.state.find(child.into())
    }

    /// Look up a child by pid or by handle, only if it is still running.
    pub async fn get_running<'a>(
        &self,
        child: impl Into<ChildSelector<'a>>,
    ) -> Option<SharedChild> {
        self.state
            .find(child.into())
            .filter(|tracked| tracked.is_running())
            .map(|tracked| tracked.child)
    }

    /// Return every tracked child, running or exited.
    pub async fn list(&self) -> Vec<TrackedChild> {
        self.state.read_children().values().cloned().collect()
    }

    /// Return every child that is still running.
    pub async fn list_running(&self) -> Vec<TrackedChild> {
        self.state.running_children()
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

    /// Send a signal to a tracked child, selected by pid or by handle, and
    /// its descendants when [`ProcessRegistryOptions::signal_descendants`]
    /// is set, without waiting for it to exit. Returns false if the child
    /// is not tracked or has already exited.
    pub async fn signal<'a>(
        &self,
        child: impl Into<ChildSelector<'a>>,
        signal: SignalType,
    ) -> io::Result<bool> {
        let Some(child) = self.get_running(child).await else {
            return Ok(false);
        };

        self.state.signal_tree(&child, signal).await?;

        Ok(true)
    }

    /// Send a signal to every running child, and their descendants when
    /// configured, without waiting or escalating. Use [`Self::shutdown`]
    /// for a graceful shutdown with escalation.
    pub async fn signal_running(&self, signal: SignalType) {
        self.state
            .signal_all(&self.state.running_children(), signal)
            .await;
    }

    /// Force kill a tracked child, selected by pid or by handle, and its
    /// descendants, and wait for it to exit. Returns `None` if the child
    /// is not tracked.
    pub async fn kill<'a>(
        &self,
        child: impl Into<ChildSelector<'a>>,
    ) -> io::Result<Option<ChildExit>> {
        let Some(tracked) = self.get(child).await else {
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
    /// Children stay tracked until released.
    ///
    /// When the background tasks are running, this takes the same path as
    /// an OS signal. Like a repeated OS signal, calling this while another
    /// shutdown is in its grace period escalates that shutdown to a force
    /// kill. Otherwise the shutdown runs inline.
    pub async fn shutdown(&self, signal: SignalType) {
        if self.is_started() {
            let pids = self
                .state
                .running_children()
                .iter()
                .map(|tracked| tracked.pid())
                .collect::<Vec<_>>();

            let mut events = self.state.events.subscribe();

            // The coordinator subscribes when started, so a send only fails
            // if it has died, in which case run the shutdown ourselves.
            if self.state.signals.send(signal).is_ok() {
                // Whether the signal starts a shutdown or escalates one in
                // progress, it ends with these children gone, so wait on
                // them rather than on a shutdown that may not be ours. The
                // exception is a child that survives a force kill, where
                // the coordinator gives up, so a forced shutdown that
                // finishes ends the wait too.
                let exited = self.state.wait_until_exited(Some(&pids));
                let mut forced = false;

                tokio::pin!(exited);

                loop {
                    tokio::select! {
                        _ = &mut exited => return,
                        event = events.recv() => match event {
                            Ok(ProcessEvent::ShutdownForced { .. }) => forced = true,
                            Ok(ProcessEvent::ShutdownFinished) if forced => return,
                            Err(broadcast::error::RecvError::Closed) => return,
                            _ => {}
                        },
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
}

impl Drop for ProcessRegistry {
    fn drop(&mut self) {
        self.stop();

        // Nothing can await here, so signal synchronously. The children are
        // snapshotted first, so that the lock isn't held while the process
        // table is read.
        let running = self.state.running_children();

        let trees = if self.state.options.signal_descendants {
            let pids = running.iter().map(|c| c.pid()).collect::<Vec<_>>();

            tree::descendants_of(&pids)
        } else {
            vec![]
        };

        self.state.send_signals(&running, trees, SignalType::Kill);

        // Stop the readers of every tracked child, exited ones included,
        // so nothing waits on a pipe held by a descendant
        self.state.stop_all_output();
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
