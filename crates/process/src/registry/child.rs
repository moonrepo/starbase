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

/// Selects a tracked child, either by pid or by handle. Anything that takes
/// a selector accepts a `u32` pid or a `&SharedChild` directly.
///
/// A pid selects whichever child is currently tracked under it. A handle
/// selects only that exact child, so a stale handle never selects a newer
/// child that the OS gave the same pid. Prefer a handle when you have one.
#[derive(Clone, Copy)]
pub enum ChildSelector<'a> {
    /// The child currently tracked under this pid.
    Pid(u32),

    /// The child this handle, or any clone of it, refers to.
    Handle(&'a SharedChild),
}

impl ChildSelector<'_> {
    /// Return the pid of the selected child.
    pub fn pid(&self) -> u32 {
        match self {
            Self::Pid(pid) => *pid,
            Self::Handle(child) => child.id(),
        }
    }

    /// Return true if this selects the given tracked child.
    pub fn matches(&self, tracked: &TrackedChild) -> bool {
        match self {
            Self::Pid(pid) => tracked.pid() == *pid,
            Self::Handle(child) => tracked.child.same_child(child),
        }
    }
}

impl From<u32> for ChildSelector<'_> {
    fn from(pid: u32) -> Self {
        Self::Pid(pid)
    }
}

impl<'a> From<&'a SharedChild> for ChildSelector<'a> {
    fn from(child: &'a SharedChild) -> Self {
        Self::Handle(child)
    }
}
