//! Registry configuration, process identities, and lifecycle events.

use crate::{ChildExit, SignalType};
use std::io;
use std::sync::Arc;
use std::time::Duration;

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
pub struct ProcessId(pub(super) u64);

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
