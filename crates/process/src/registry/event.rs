use crate::shared_child::ChildExit;
use crate::signal::SignalType;

/// Something that happened in the registry. Subscribe with
/// [`ProcessRegistry::subscribe_events`](super::ProcessRegistry::subscribe_events).
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
