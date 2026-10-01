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
//! leave the group cannot be managed. Windows children start suspended and join
//! a job before being resumed. Windows interrupts are unsupported; other signals
//! terminate the job immediately.
//!
//! Captured output is bounded per stream. Stdin and both output streams run
//! concurrently, and pipes inherited by escaped descendants have a drain timeout.
//! This API captures bytes; console streaming and interactive stdin remain the
//! responsibility of the existing execution APIs.
//!
//! ```no_run
//! use starbase_process::{Command, registry::ProcessRegistry};
//! # async fn example() -> miette::Result<()> {
//! let registry = ProcessRegistry::new();
//! let mut command = Command::<starbase_console::EmptyReporter>::new("rustc");
//! command.arg("--version").no_shell().set_cache(true);
//! let output = registry.execute(&mut command).await?;
//! registry.shutdown().await.map_err(miette::Report::msg)?;
//! # Ok(()) }
//! ```

mod cache;
mod command;
mod lifecycle;
mod process;
mod signals;
mod supervisor;
mod types;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

pub use process::ProcessHandle;
pub use types::{ProcessId, RegistryEvent, RegistryOptions, RegistryState};

use self::cache::CacheKey;
use self::command::Prepared;
use self::lifecycle::{Core, Owner, State};
use self::process::{Process, signal_processes};
use self::supervisor::{Supervisor, supervise};
use crate::{Command, Output, SharedChild, SignalType};
use miette::IntoDiagnostic;
use starbase_console::Reporter;
use std::collections::{HashMap, VecDeque};
use std::io;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use tokio::sync::{broadcast, watch};

/// Independent by default; `instance` provides an optional global registry.
/// Requires a live Tokio runtime with I/O and time enabled for async operations.
#[derive(Clone)]
pub struct ProcessRegistry {
    owner: Arc<Owner>,
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
        let job = match windows::Job::new(&child) {
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
}

fn ensure_running(state: &State) -> io::Result<()> {
    if state.phase != RegistryState::Running {
        return Err(io::Error::other("process registry is not accepting work"));
    }
    Ok(())
}
