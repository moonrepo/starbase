use std::time::Duration;

/// Behavioral settings for a [`ProcessRegistry`](super::ProcessRegistry).
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
    /// [`ProcessRegistry::track`](super::ProcessRegistry::track) is dropped without being released. The
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
