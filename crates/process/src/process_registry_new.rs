//! An opt-in, Tokio-first process registry. Existing `Command::exec_*` methods
//! continue to use the original registry; use this module's `spawn` or `execute`.
//!
//! A registry owns its children, even if all their result handles are dropped.
//! `stop` pauses admissions, `start` resumes them, and `shutdown` terminates and
//! reaps children before returning. Cache entries survive all three operations.
//! Clones share ownership; dropping the last registry requests immediate cleanup.
//! Call `shutdown` before dropping the Tokio runtime to guarantee reaping.
//!
//! Each command gets a Unix process group or a Windows job. Remaining descendants
//! are terminated when the direct child exits. Unix descendants that deliberately
//! leave the group cannot be managed. Windows children start suspended and join a job before being resumed. Windows interrupts are
//! unsupported; other signals terminate the job immediately.
//!
//! Captured output is bounded per stream. Stdin and both output streams run
//! concurrently, and pipes inherited by escaped descendants have a drain timeout.
//! This API captures bytes; console streaming and interactive stdin remain the
//! responsibility of the existing execution APIs.
//!
//! ```no_run
//! use starbase_process::{Command, process_registry_new::ProcessRegistry};
//! # async fn example() -> miette::Result<()> {
//! let registry = ProcessRegistry::new();
//! let mut command = Command::<starbase_console::EmptyReporter>::new("rustc");
//! command.arg("--version").no_shell().set_cache(true);
//! let output = registry.execute(&mut command).await?;
//! registry.shutdown().await.map_err(miette::Report::msg)?;
//! # Ok(()) }
//! ```

use crate::{ChildExit, Command, Output, SharedChild, SignalType};
use bytes::Bytes;
use miette::IntoDiagnostic;
use starbase_console::Reporter;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;

/// Resource bounds and shutdown policy for one registry.
#[derive(Clone, Debug)]
pub struct RegistryOptions {
    /// Time allowed for graceful termination before sending `Kill`.
    pub shutdown_grace: Duration,
    /// Maximum wait for inherited pipes after the direct child exits.
    pub output_drain_timeout: Duration,
    /// Maximum captured bytes per output stream. Exceeding it kills the process.
    pub max_output_bytes: usize,
    /// Maximum number of cached successful outputs. Zero disables caching.
    pub cache_capacity: usize,
    /// Maximum total stdout and stderr bytes retained by the cache.
    pub cache_max_bytes: usize,
    /// Time until a cache entry expires. Files and network state are not tracked.
    pub cache_ttl: Duration,
}

impl Default for RegistryOptions {
    fn default() -> Self {
        Self {
            shutdown_grace: Duration::from_secs(5),
            output_drain_timeout: Duration::from_secs(1),
            max_output_bytes: 16 * 1024 * 1024,
            cache_capacity: 128,
            cache_max_bytes: 64 * 1024 * 1024,
            cache_ttl: Duration::from_secs(300),
        }
    }
}

/// Admission and shutdown state. A stopped registry still supervises children.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistryState {
    /// New processes can be spawned.
    Running,
    /// New processes are rejected; existing processes continue.
    Stopped,
    /// New processes are rejected while children are being terminated.
    ShuttingDown,
}

/// Identity within a registry, independent of OS PID reuse.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProcessId(u64);

/// Best-effort notifications. Slow subscribers may receive `RecvError::Lagged`;
/// process handles provide durable completion results independently of events.
#[derive(Clone, Debug)]
pub enum RegistryEvent {
    /// Admission state changed.
    StateChanged(RegistryState),
    /// A process was started.
    Spawned { id: ProcessId, pid: u32 },
    /// A process and its I/O completed successfully (possibly with nonzero exit).
    Completed { id: ProcessId, exit: ChildExit },
    /// Supervision, capture, or cleanup failed.
    Failed {
        id: ProcessId,
        error: Arc<io::Error>,
    },
    /// A requested signal was delivered.
    Signalled { id: ProcessId, signal: SignalType },
    /// An OS signal was received by the opt-in listener.
    SignalReceived(SignalType),
}

type Completion = Result<Output, Arc<io::Error>>;
type ShutdownResult = Result<(), Arc<io::Error>>;
type ShutdownReceiver = watch::Receiver<Option<ShutdownResult>>;

/// A durable, cloneable result handle. Dropping it does not cancel the process.
/// Use `signal` to terminate it, or shut down the registry that owns it.
#[derive(Clone)]
pub struct ProcessHandle {
    process: Arc<Process>,
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

/// Independent by default; `instance` provides an optional global registry.
/// Requires a live Tokio runtime with I/O and time enabled for async operations.
#[derive(Clone)]
pub struct ProcessRegistry {
    owner: Arc<Owner>,
}

struct Owner {
    core: Arc<Core>,
    listener: Mutex<Option<JoinHandle<()>>>,
}

struct Core {
    options: RegistryOptions,
    state: Mutex<State>,
    events: broadcast::Sender<RegistryEvent>,
    changes: watch::Sender<u64>,
}

struct State {
    phase: RegistryState,
    next_id: u64,
    running: HashMap<ProcessId, Arc<Process>>,
    cache: VecDeque<CacheEntry>,
    cache_bytes: usize,
    cache_epoch: u64,
    shutdown: Option<ShutdownReceiver>,
}

// Synchronous locks protect short state transitions only. No guard crosses an
// await, and poison recovery permits Drop cleanup after an unrelated panic.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

fn shared_error(error: Arc<io::Error>) -> io::Error {
    io::Error::new(error.kind(), error)
}

impl Default for ProcessRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessRegistry {
    /// Create an independent registry with default bounds. Installs no signals.
    pub fn new() -> Self {
        Self::with_options(RegistryOptions::default())
    }

    /// Create an independent registry with explicit resource limits.
    pub fn with_options(options: RegistryOptions) -> Self {
        Self {
            owner: Arc::new(Owner {
                core: Arc::new(Core {
                    options,
                    state: Mutex::new(State {
                        phase: RegistryState::Running,
                        next_id: 1,
                        running: HashMap::new(),
                        cache: VecDeque::new(),
                        cache_bytes: 0,
                        cache_epoch: 0,
                        shutdown: None,
                    }),
                    events: broadcast::channel(256).0,
                    changes: watch::channel(0).0,
                }),
                listener: Mutex::new(None),
            }),
        }
    }

    /// Return the optional process-wide instance. Explicit shutdown is required
    /// because static values are not dropped at program exit.
    pub fn instance() -> Self {
        static INSTANCE: OnceLock<ProcessRegistry> = OnceLock::new();
        INSTANCE.get_or_init(Self::new).clone()
    }

    /// Subscribe to future lifecycle events without affecting supervision.
    pub fn subscribe(&self) -> broadcast::Receiver<RegistryEvent> {
        self.owner.core.events.subscribe()
    }

    /// Read the current admission state.
    pub fn state(&self) -> RegistryState {
        lock(&self.owner.core.state).phase
    }

    /// Snapshot handles for processes still being supervised, including I/O drain.
    pub fn running(&self) -> Vec<ProcessHandle> {
        lock(&self.owner.core.state)
            .running
            .values()
            .map(|process| process.handle())
            .collect()
    }

    /// Reject new work while preserving supervision and cache entries.
    pub fn stop(&self) -> io::Result<()> {
        self.set_state(RegistryState::Stopped)
    }

    /// Resume admissions, including after a completed shutdown.
    pub fn start(&self) -> io::Result<()> {
        self.set_state(RegistryState::Running)
    }

    /// Resume admissions without replacing state or interrupting running work.
    /// Supervisors remain active during `stop`, so there is no worker to rebuild.
    pub fn restart(&self) -> io::Result<()> {
        self.start()
    }

    fn set_state(&self, phase: RegistryState) -> io::Result<()> {
        let core = &self.owner.core;
        let mut state = lock(&core.state);
        if state.phase == RegistryState::ShuttingDown {
            return Err(io::Error::other("registry shutdown is in progress"));
        }
        if state.phase != phase {
            state.phase = phase;
            let _ = core.events.send(RegistryEvent::StateChanged(phase));
        }
        Ok(())
    }

    /// Discard cached results. Already running commands will not repopulate the
    /// cache with results from before this invalidation.
    pub fn clear_cache(&self) {
        let mut state = lock(&self.owner.core.state);
        state.cache.clear();
        state.cache_bytes = 0;
        state.cache_epoch = state.cache_epoch.wrapping_add(1);
    }

    /// Spawn and supervise a command, capturing output. Always starts a process;
    /// `execute` additionally supports the command's cache and nonzero policy.
    /// Stdin is closed after buffered input (or immediately if there is none).
    /// Errors include stopped admissions, invalid commands, and spawn failures.
    pub async fn spawn<R: Reporter>(&self, command: &Command<R>) -> miette::Result<ProcessHandle> {
        let prepared = Prepared::new(command)?;
        self.spawn_prepared(command, prepared, None)
    }

    /// Run with optional output caching and `Command::error_on_nonzero` handling.
    /// Cache keys use the resolved program, args, full environment snapshot,
    /// absolute working directory, and actual stdin bytes. Successful results only
    /// are cached. Concurrent identical misses may run independently.
    pub async fn execute<R: Reporter>(&self, command: &mut Command<R>) -> miette::Result<Output> {
        let prepared = Prepared::new(command)?;
        let cache = if command.should_cache_output() {
            let mut state = lock(&self.owner.core.state);
            ensure_running(&state).into_diagnostic()?;
            if let Some(output) = state.cached(&prepared.key, &self.owner.core.options) {
                return command.handle_cached_output(output);
            }
            Some((prepared.key.clone(), state.cache_epoch))
        } else {
            None
        };
        let handle = self.spawn_prepared(command, prepared, cache)?;
        let output = handle.wait().await.into_diagnostic()?;
        command.handle_cached_output(output)
    }

    fn spawn_prepared<R: Reporter>(
        &self,
        command: &Command<R>,
        mut prepared: Prepared,
        cache: Option<(CacheKey, u64)>,
    ) -> miette::Result<ProcessHandle> {
        let runtime = tokio::runtime::Handle::try_current().into_diagnostic()?;
        // Register SIGCHLD before spawn to avoid missing a very short-lived child.
        #[cfg(unix)]
        let child_signals = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::child())
            .into_diagnostic()?;
        let core = &self.owner.core;
        // Admission, OS spawn, registration and worker handoff are one transition.
        // There is no cancellation point at which an unowned child can escape.
        let mut state = lock(&core.state);
        ensure_running(&state).into_diagnostic()?;
        let id = ProcessId(state.next_id);
        state.next_id = state
            .next_id
            .checked_add(1)
            .ok_or_else(|| miette::miette!("process identity space exhausted"))?;
        let child = prepared.command.spawn().into_diagnostic()?;
        #[cfg(windows)]
        let job = match windows_job::Job::new(&child) {
            Ok(job) => job,
            Err(error) => {
                // kill_on_drop also covers attachment or resume failures.
                drop(child);
                return Err(error).into_diagnostic();
            }
        };
        let process = Arc::new(Process {
            id,
            child: SharedChild::new(child),
            active: Mutex::new(true),
            result: watch::channel(None).0,
            events: core.events.clone(),
            #[cfg(windows)]
            job,
        });
        let handle = process.handle();
        state.running.insert(id, process.clone());
        let _ = core.events.send(RegistryEvent::Spawned {
            id,
            pid: handle.pid(),
        });
        let guard = Supervisor {
            core: core.clone(),
            process: process.clone(),
            finished: false,
        };
        runtime.spawn(async move {
            supervise(
                guard,
                prepared.input,
                cache,
                #[cfg(unix)]
                child_signals,
            )
            .await;
        });
        drop(state);
        command.pre_log_command(&process.child);
        Ok(handle)
    }

    /// Signal every currently registered tree, attempting all even if one fails.
    /// Returns the first delivery error.
    pub fn signal_all(&self, signal: SignalType) -> io::Result<()> {
        signal_processes(self.running().iter().map(|handle| &handle.process), signal)
    }

    /// Wait for the registry to become idle. Stop admissions first if other tasks
    /// may spawn work. Cancelling this wait has no effect on child lifetimes.
    pub async fn wait_for_idle(&self) {
        self.owner.core.wait_for_idle().await;
    }

    /// Reject new work, send termination, escalate after the grace period, and
    /// await all supervisors. Concurrent callers share the same shutdown result.
    /// Cancelling the caller does not cancel shutdown. Ends in `Stopped`; cache
    /// and event subscriptions survive. Call `start` to accept work again.
    pub async fn shutdown(&self) -> io::Result<()> {
        let mut done = self.owner.core.begin_shutdown(SignalType::Terminate)?;
        loop {
            if let Some(result) = done.borrow_and_update().as_ref() {
                return result.clone().map_err(shared_error);
            }
            done.changed()
                .await
                .map_err(|_| io::Error::other("shutdown task stopped"))?;
        }
    }

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

fn ensure_running(state: &State) -> io::Result<()> {
    if state.phase != RegistryState::Running {
        return Err(io::Error::other("process registry is not accepting work"));
    }
    Ok(())
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

fn signal_processes<'a>(
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

impl Core {
    async fn wait_for_idle(&self) {
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

    fn begin_shutdown(
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

#[derive(Clone, Eq, PartialEq)]
struct CacheKey {
    program: OsString,
    args: Vec<OsString>,
    cwd: PathBuf,
    env: BTreeMap<OsString, OsString>,
    input: Vec<u8>,
}

struct CacheEntry {
    key: CacheKey,
    output: Output,
    inserted: Instant,
}

impl State {
    fn cached(&mut self, key: &CacheKey, options: &RegistryOptions) -> Option<Output> {
        self.cache
            .retain(|entry| entry.inserted.elapsed() < options.cache_ttl);
        self.cache_bytes = self
            .cache
            .iter()
            .map(|entry| output_size(&entry.output))
            .sum();
        let index = self.cache.iter().position(|entry| &entry.key == key)?;
        let entry = self.cache.remove(index)?;
        let output = entry.output.clone();
        self.cache.push_back(entry);
        Some(output)
    }

    fn cache(&mut self, key: CacheKey, epoch: u64, output: &Output, options: &RegistryOptions) {
        let size = output_size(output);
        if epoch != self.cache_epoch
            || !output.success()
            || options.cache_capacity == 0
            || size > options.cache_max_bytes
            || options.cache_ttl.is_zero()
        {
            return;
        }
        self.cache
            .retain(|entry| entry.key != key && entry.inserted.elapsed() < options.cache_ttl);
        self.cache_bytes = self
            .cache
            .iter()
            .map(|entry| output_size(&entry.output))
            .sum();
        while self.cache.len() >= options.cache_capacity
            || self.cache_bytes > options.cache_max_bytes - size
        {
            let Some(entry) = self.cache.pop_front() else {
                break;
            };
            self.cache_bytes -= output_size(&entry.output);
        }
        self.cache_bytes += size;
        self.cache.push_back(CacheEntry {
            key,
            output: output.clone(),
            inserted: Instant::now(),
        });
    }
}

fn output_size(output: &Output) -> usize {
    output.stdout.len().saturating_add(output.stderr.len())
}

struct Prepared {
    command: tokio::process::Command,
    input: Vec<u8>,
    key: CacheKey,
}

impl Prepared {
    fn new<R: Reporter>(command: &Command<R>) -> miette::Result<Self> {
        let mut native = command.create_sync_command()?;
        let cwd = std::env::current_dir().into_diagnostic()?;
        let cwd = native
            .get_current_dir()
            .map_or(cwd.clone(), |path| cwd.join(path));
        let mut env = std::env::vars_os().collect::<BTreeMap<_, _>>();
        for (key, value) in native.get_envs() {
            #[cfg(windows)]
            env.retain(|existing, _| {
                !existing
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&key.to_string_lossy())
            });
            match value {
                Some(value) => {
                    env.insert(key.to_owned(), value.to_owned());
                }
                None => {
                    env.remove(key);
                }
            }
        }
        // Freeze inherited values used in the key and by the actual child.
        native.env_clear().envs(&env).current_dir(&cwd);
        let input = if command.continuous_pipe {
            command
                .input
                .iter()
                .flat_map(|value| value.as_encoded_bytes())
                .copied()
                .collect()
        } else {
            command
                .input
                .join(OsStr::new(" "))
                .as_encoded_bytes()
                .to_vec()
        };
        let key = CacheKey {
            program: native.get_program().to_owned(),
            args: native.get_args().map(OsStr::to_owned).collect(),
            cwd,
            env,
            input: input.clone(),
        };
        let mut native = tokio::process::Command::from(native);
        native
            .kill_on_drop(true)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        native.process_group(0);
        #[cfg(windows)]
        native.creation_flags(windows_sys::Win32::System::Threading::CREATE_SUSPENDED);
        Ok(Self {
            command: native,
            input,
            key,
        })
    }
}

struct Process {
    id: ProcessId,
    child: SharedChild,
    // On Unix this remains true until group cleanup BEFORE reaping, pinning the
    // group leader PID against reuse. No public API exposes the SharedChild.
    active: Mutex<bool>,
    result: watch::Sender<Option<Completion>>,
    events: broadcast::Sender<RegistryEvent>,
    #[cfg(windows)]
    job: windows_job::Job,
}

impl Process {
    fn handle(self: &Arc<Self>) -> ProcessHandle {
        ProcessHandle {
            process: self.clone(),
            result: self.result.subscribe(),
        }
    }

    fn signal(&self, signal: SignalType) -> io::Result<()> {
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

    fn finish_tree(&self) -> io::Result<()> {
        let mut active = lock(&self.active);
        if *active {
            self.signal_tree(SignalType::Kill)?;
            *active = false;
        }
        Ok(())
    }
}

struct Supervisor {
    core: Arc<Core>,
    process: Arc<Process>,
    finished: bool,
}

impl Supervisor {
    fn finish(&mut self, result: io::Result<Output>, cache: Option<(CacheKey, u64)>) {
        let result = result.map_err(Arc::new);
        let mut state = lock(&self.core.state);
        if let (Ok(output), Some((key, epoch))) = (&result, cache) {
            state.cache(key, epoch, output, &self.core.options);
        }
        state.running.remove(&self.process.id);
        let event = match &result {
            Ok(output) => RegistryEvent::Completed {
                id: self.process.id,
                exit: output.exit.clone(),
            },
            Err(error) => RegistryEvent::Failed {
                id: self.process.id,
                error: error.clone(),
            },
        };
        self.process.result.send_replace(Some(result));
        let _ = self.core.events.send(event);
        self.core
            .changes
            .send_modify(|version| *version = version.wrapping_add(1));
        self.finished = true;
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        if !self.finished {
            if let Err(error) = self.process.finish_tree() {
                tracing::warn!(%error, "Failed to clean up cancelled process supervisor");
            }
            self.process.child.stop_output();
            self.finish(
                Err(io::Error::other("process supervisor was cancelled")),
                None,
            );
        }
    }
}

async fn supervise(
    mut guard: Supervisor,
    input: Vec<u8>,
    cache: Option<(CacheKey, u64)>,
    #[cfg(unix)] mut child_signals: tokio::signal::unix::Signal,
) {
    let process = guard.process.clone();
    let child = &process.child;
    let mut stdin = child.take_stdin().await;
    let stdout = child.take_stdout().await;
    let stderr = child.take_stderr().await;
    let limit = guard.core.options.max_output_bytes;
    let result = {
        let io = async {
            let write = async {
                if let Some(mut stdin) = stdin.take() {
                    tokio::select! {
                        _ = child.wait_till_output_stopped() => {},
                        result = stdin.write_all(&input) => {
                            if let Err(error) = result && error.kind() != io::ErrorKind::BrokenPipe {
                                return Err(error);
                            }
                        }
                    }
                }
                Ok::<_, io::Error>(())
            };
            let (_, stdout, stderr) = tokio::try_join!(
                write,
                read_output(stdout, child, limit),
                read_output(stderr, child, limit)
            )?;
            Ok::<_, io::Error>((stdout, stderr))
        };
        let wait = async {
            #[cfg(unix)]
            wait_unreaped(&process, &mut child_signals).await?;
            #[cfg(unix)]
            process.finish_tree()?;
            let exit = child.wait().await?;
            #[cfg(windows)]
            process.finish_tree()?;
            Ok::<_, io::Error>(exit)
        };
        tokio::pin!(io, wait);
        tokio::select! {
            result = &mut io => match result {
                Ok((stdout, stderr)) => wait.await.map(|exit| Output { exit, stdout, stderr }),
                Err(error) => Err(error),
            },
            result = &mut wait => match result {
                Ok(exit) => match tokio::time::timeout(guard.core.options.output_drain_timeout, &mut io).await {
                    Ok(result) => result.map(|(stdout, stderr)| Output { exit, stdout, stderr }),
                    Err(_) => Err(io::Error::new(io::ErrorKind::TimedOut, "process output pipes did not close after exit")),
                },
                Err(error) => Err(error),
            },
        }
    };
    if result.is_err() {
        if let Err(error) = process.finish_tree() {
            tracing::warn!(%error, "Failed to terminate process tree after supervision error");
        }
        child.stop_output();
        // Always reap, including on capture errors and output limits.
        if let Err(error) = child.wait().await {
            tracing::warn!(%error, "Failed to reap process after supervision error");
        }
    }
    guard.finish(result, cache);
}

async fn read_output<R: AsyncRead + Unpin>(
    reader: Option<R>,
    child: &SharedChild,
    limit: usize,
) -> io::Result<Bytes> {
    let Some(mut reader) = reader else {
        return Ok(Bytes::new());
    };
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let size = tokio::select! {
            biased;
            _ = child.wait_till_output_stopped() => break,
            result = reader.read(&mut buffer) => result?,
        };
        if size == 0 {
            break;
        }
        if size > limit.saturating_sub(bytes.len()) {
            return Err(io::Error::other(
                "process output exceeded configured byte limit",
            ));
        }
        bytes.extend_from_slice(&buffer[..size]);
    }
    Ok(Bytes::from(bytes))
}

#[cfg(target_vendor = "apple")]
fn apple_group_has_only_zombies(pgid: u32) -> io::Result<bool> {
    const PROC_PGRP_ONLY: u32 = 2;
    let mut pids = vec![0_i32; 32];
    loop {
        let bytes =
            i32::try_from(std::mem::size_of_val(pids.as_slice())).map_err(io::Error::other)?;
        // SAFETY: the buffer is writable for `bytes` bytes. This only inspects
        // the private process group while its unreaped leader pins the group ID.
        let written =
            unsafe { libc::proc_listpids(PROC_PGRP_ONLY, pgid, pids.as_mut_ptr().cast(), bytes) };
        if written <= 0 {
            return Err(io::Error::last_os_error());
        }
        if written == bytes {
            pids.resize(pids.len() * 2, 0);
            continue;
        }
        pids.truncate(written as usize / std::mem::size_of::<i32>());
        break;
    }
    for pid in pids {
        // SAFETY: a zeroed proc_bsdinfo and its exact size are passed to libproc.
        let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of_val(&info) as i32;
        let read = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDTBSDINFO,
                0,
                (&mut info as *mut libc::proc_bsdinfo).cast(),
                size,
            )
        };
        if read != size {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::ESRCH) {
                continue;
            }
            return Err(error);
        }
        if info.pbi_pgid == pgid && info.pbi_status != libc::SZOMB {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(unix)]
async fn wait_unreaped(
    process: &Process,
    signals: &mut tokio::signal::unix::Signal,
) -> io::Result<()> {
    loop {
        match has_exited_unreaped(process.child.id()) {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => {
                // Someone outside this API reaped the PID. Never signal it again.
                if error.raw_os_error() == Some(libc::ECHILD) {
                    *lock(&process.active) = false;
                }
                return Err(error);
            }
        }
        if signals.recv().await.is_none() {
            return Err(io::Error::other("SIGCHLD listener closed"));
        }
    }
}

#[cfg(unix)]
fn has_exited_unreaped(pid: u32) -> io::Result<bool> {
    // SAFETY: zero is a valid initial siginfo_t representation; waitid writes it
    // on success. WNOWAIT observes exit without releasing the PID. Only this
    // supervisor owns the child's wait API, including during signalling.
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    let result = unsafe {
        libc::waitid(
            libc::P_PID,
            pid as libc::id_t,
            &mut info,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    if result == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: waitid initialized info and si_pid is valid for this event.
    Ok(unsafe { info.si_pid() } != 0)
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

#[cfg(windows)]
mod windows_job {
    use std::io;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::System::JobObjects::*;

    // Tokio does not expose the primary thread handle. Since CREATE_SUSPENDED
    // prevents the child from executing any user code, it has exactly one thread;
    // find it by its pinned owner PID and resume only after successful assignment.
    fn resume_child(pid: u32) -> io::Result<()> {
        use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
        use windows_sys::Win32::System::Diagnostics::ToolHelp::*;
        use windows_sys::Win32::System::Threading::{
            GetProcessIdOfThread, OpenThread, ResumeThread, THREAD_QUERY_LIMITED_INFORMATION,
            THREAD_SUSPEND_RESUME,
        };
        // SAFETY: the flags request a system thread snapshot; the returned handle
        // is checked before transferring ownership to OwnedHandle.
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let snapshot = unsafe { OwnedHandle::from_raw_handle(snapshot) };
        let mut entry = THREADENTRY32 {
            dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
            ..Default::default()
        };
        // SAFETY: entry has the required layout and initialized size.
        let mut found = unsafe { Thread32First(snapshot.as_raw_handle(), &mut entry) };
        while found != 0 {
            if entry.th32OwnerProcessID == pid {
                // SAFETY: open a handle before acting on the thread. Validate its
                // owner below in case an external kill caused thread ID reuse.
                let thread = unsafe {
                    OpenThread(
                        THREAD_SUSPEND_RESUME | THREAD_QUERY_LIMITED_INFORMATION,
                        0,
                        entry.th32ThreadID,
                    )
                };
                if thread.is_null() {
                    return Err(io::Error::last_os_error());
                }
                let thread = unsafe { OwnedHandle::from_raw_handle(thread) };
                // SAFETY: the live handle pins the thread identity across both calls.
                if unsafe { GetProcessIdOfThread(thread.as_raw_handle()) } != pid {
                    return Err(io::Error::other("suspended child thread identity changed"));
                }
                if unsafe { ResumeThread(thread.as_raw_handle()) } == u32::MAX {
                    return Err(io::Error::last_os_error());
                }
                return Ok(());
            }
            found = unsafe { Thread32Next(snapshot.as_raw_handle(), &mut entry) };
        }
        Err(io::Error::other("suspended child thread was not found"))
    }

    pub(super) struct Job(OwnedHandle);

    impl Job {
        pub(super) fn new(child: &tokio::process::Child) -> io::Result<Self> {
            // SAFETY: null security/name pointers select an unnamed, non-inherited
            // job. Every successfully returned handle is owned and closed once.
            let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            let job = Self(unsafe { OwnedHandle::from_raw_handle(handle) });
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let child_handle = child
                .raw_handle()
                .ok_or_else(|| io::Error::other("child handle unavailable"))?;
            // SAFETY: handles are live; info has the required layout and size.
            if unsafe {
                SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    std::mem::size_of_val(&info) as u32,
                )
            } == 0
                || unsafe { AssignProcessToJobObject(handle, child_handle) } == 0
            {
                return Err(io::Error::last_os_error());
            }
            resume_child(
                child
                    .id()
                    .ok_or_else(|| io::Error::other("child PID unavailable"))?,
            )?;
            Ok(job)
        }

        pub(super) fn terminate(&self) -> io::Result<()> {
            // SAFETY: this object owns the live job handle for the call's duration.
            if unsafe { TerminateJobObject(self.0.as_raw_handle(), 1) } == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
    }
}
