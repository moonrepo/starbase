use crate::shared_child::{ChildExit, SharedChild};
use std::time::Instant;

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
