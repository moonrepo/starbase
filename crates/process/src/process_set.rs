use crate::shared_child::SharedChild;
use crate::signal::SignalType;
use crate::signal_listener;
use rustc_hash::FxHashMap;
use std::fmt;
use std::io;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;
use tokio::process::{Child, Command as TokioCommand};
use tokio::sync::{Notify, mpsc};
use tokio::task::JoinSet;
use tokio::time::sleep;
use tracing::{debug, warn};

static ROOT: OnceLock<ProcessSet> = OnceLock::new();

/// A set of running child processes, owned by the code that starts them.
/// Sets nest under one process-wide [`root`](Self::root). An OS
/// termination signal shuts the root down along with every nested set,
/// and a nested set can also be shut down on its own. Clones share the
/// same set.
#[derive(Clone)]
pub struct ProcessSet {
    inner: Arc<SetInner>,
}

pub(crate) struct SetInner {
    grace: Mutex<Duration>,

    // Shared by every set in the tree, so waiting on a set also wakes
    // when one of its nested sets goes idle
    idle: Arc<Notify>,

    state: Mutex<SetState>,
}

#[derive(Default)]
struct SetState {
    children: FxHashMap<u32, SharedChild>,
    nested: Vec<Weak<SetInner>>,

    // Hands another shutdown request to the shutdown in progress
    shutdown: Option<mpsc::UnboundedSender<SignalType>>,
}

impl ProcessSet {
    /// Return the process-wide root set. OS termination signals shut it
    /// down, along with every set nested under it. Its grace period is 2
    /// seconds unless changed with [`Self::configure_root`].
    pub fn root() -> ProcessSet {
        ROOT.get_or_init(|| {
            // Children tracked here must be shut down on an OS signal
            signal_listener::start();

            Self::create(Duration::from_secs(2), Arc::new(Notify::new()))
        })
        .clone()
    }

    pub(crate) fn try_root() -> Option<ProcessSet> {
        ROOT.get().cloned()
    }

    /// Set the root's grace period. Sets nested afterwards inherit it.
    /// A zero duration waits indefinitely instead of force killing.
    pub fn configure_root(grace: Duration) {
        *Self::root().inner.grace.lock().unwrap() = grace;
    }

    /// Create a set nested under this one, with the same grace period. It's
    /// shut down along with its parent, or on its own.
    pub fn nested(&self) -> ProcessSet {
        self.nested_with_grace(self.grace())
    }

    pub(crate) fn nested_with_grace(&self, grace: Duration) -> ProcessSet {
        let set = Self::create(grace, Arc::clone(&self.inner.idle));
        let mut state = self.inner.state.lock().unwrap();

        state.nested.retain(|nested| nested.strong_count() > 0);
        state.nested.push(Arc::downgrade(&set.inner));

        set
    }

    fn create(grace: Duration, idle: Arc<Notify>) -> ProcessSet {
        ProcessSet {
            inner: Arc::new(SetInner {
                grace: Mutex::new(grace),
                idle,
                state: Mutex::new(SetState::default()),
            }),
        }
    }

    /// Return how long children get to exit after a shutdown signal before
    /// they're force killed. Zero waits indefinitely instead.
    pub fn grace(&self) -> Duration {
        *self.inner.grace.lock().unwrap()
    }

    /// Spawn the command and track the child in this set. The returned
    /// handle owns the child: dropping it while the child still runs kills
    /// the child, and dropping it after the child exits stops tracking it.
    pub fn spawn(&self, command: &mut TokioCommand) -> io::Result<SharedChild> {
        Ok(self.track(command.spawn()?))
    }

    pub(crate) fn track(&self, child: Child) -> SharedChild {
        let owner = SharedChild::new_owned(child, Arc::downgrade(&self.inner));

        self.inner
            .state
            .lock()
            .unwrap()
            .children
            .insert(owner.id(), owner.clone());

        owner
    }

    pub(crate) fn get(&self, pid: u32) -> Option<SharedChild> {
        self.inner.state.lock().unwrap().children.get(&pid).cloned()
    }

    /// Stop tracking a child without signalling it, and stop its owner
    /// from killing it on drop.
    pub(crate) fn release(&self, pid: u32) {
        let removed = self.inner.state.lock().unwrap().children.remove(&pid);

        if let Some(child) = removed {
            child.stop_cleanup();
            self.inner.idle.notify_waiters();
        }
    }

    /// Send `signal` to every child in this set and its nested sets, wait
    /// the grace period, then force kill the children of this shutdown,
    /// stopping their output readers. Calling this again while a shutdown
    /// is waiting force kills every running child at once, unless the
    /// children of that shutdown have all exited, in which case it starts
    /// a new shutdown.
    pub async fn shutdown(&self, signal: SignalType) {
        let mut shutdowns = JoinSet::new();

        for set in self.subtree() {
            shutdowns.spawn(async move { set.shutdown_own(signal).await });
        }

        shutdowns.join_all().await;
    }

    /// Force kill every child in this set and its nested sets, without
    /// waiting for them to exit.
    pub fn kill_all(&self) {
        for set in self.subtree() {
            for child in set.children() {
                if let Err(error) = child.send_signal(SignalType::Kill) {
                    warn!(pid = child.id(), %error, "Failed to kill child process");
                }
            }
        }
    }

    /// Wait until no child is tracked in this set or its nested sets.
    pub async fn wait_idle(&self) {
        loop {
            let notified = self.inner.idle.notified();
            tokio::pin!(notified);

            // Register before checking, so a removal in between still wakes us
            notified.as_mut().enable();

            if self
                .subtree()
                .iter()
                .all(|set| set.inner.state.lock().unwrap().children.is_empty())
            {
                return;
            }

            notified.await;
        }
    }

    fn children(&self) -> Vec<SharedChild> {
        self.inner
            .state
            .lock()
            .unwrap()
            .children
            .values()
            .cloned()
            .collect()
    }

    fn nested_sets(&self) -> Vec<ProcessSet> {
        let mut state = self.inner.state.lock().unwrap();

        state.nested.retain(|nested| nested.strong_count() > 0);
        state
            .nested
            .iter()
            .filter_map(Weak::upgrade)
            .map(|inner| ProcessSet { inner })
            .collect()
    }

    // This set and every set nested under it, at any depth
    fn subtree(&self) -> Vec<ProcessSet> {
        let mut sets = vec![self.clone()];
        let mut index = 0;

        while index < sets.len() {
            let nested = sets[index].nested_sets();

            sets.extend(nested);
            index += 1;
        }

        sets
    }

    async fn shutdown_own(&self, signal: SignalType) {
        let Some((mut requests, _guard)) = self.start_shutdown(signal) else {
            return;
        };

        let mut next = Some(signal);

        while let Some(signal) = next.take() {
            next = self.shutdown_children(signal, &mut requests).await;

            // A request that arrived as this shutdown finished starts another
            if next.is_none() {
                let mut state = self.inner.state.lock().unwrap();

                next = requests.try_recv().ok();

                if next.is_none() {
                    state.shutdown = None;
                }
            }
        }
    }

    // Returns `None` when a shutdown is already in progress, which then
    // takes this signal as another request
    fn start_shutdown(
        &self,
        signal: SignalType,
    ) -> Option<(mpsc::UnboundedReceiver<SignalType>, ShutdownGuard<'_>)> {
        let mut state = self.inner.state.lock().unwrap();

        // Sending fails when that shutdown was cancelled, so start over
        if let Some(requests) = &state.shutdown
            && requests.send(signal).is_ok()
        {
            return None;
        }

        let (sender, receiver) = mpsc::unbounded_channel();

        state.shutdown = Some(sender.clone());

        Some((
            receiver,
            ShutdownGuard {
                inner: &self.inner,
                requests: sender,
            },
        ))
    }

    async fn shutdown_children(
        &self,
        signal: SignalType,
        requests: &mut mpsc::UnboundedReceiver<SignalType>,
    ) -> Option<SignalType> {
        let children = self.children();

        if children.is_empty() {
            return None;
        }

        let grace = self.grace();

        debug!(
            signal = ?signal,
            pids = ?children.iter().map(SharedChild::id).collect::<Vec<_>>(),
            "Shutting down {} running child processes",
            children.len()
        );

        let mut waiting = JoinSet::new();

        for child in children.clone() {
            let inner = Arc::clone(&self.inner);

            waiting.spawn(async move {
                let pid = child.id();

                if grace.is_zero() {
                    debug!(pid, "Signalling and waiting on child process");
                } else {
                    debug!(pid, "Shutting down child process");
                }

                if let Err(error) = child.kill_with_signal(signal).await {
                    warn!(
                        pid,
                        error = error.to_string(),
                        "Failed to shutdown child process"
                    );
                }

                inner.remove(&child);
            });
        }

        let repeated = if matches!(signal, SignalType::Kill) {
            Some(signal)
        } else if grace.is_zero() {
            Some(requests.recv().await.unwrap_or(SignalType::Kill))
        } else {
            tokio::select! {
                _ = sleep(grace) => None,
                signal = requests.recv() => Some(signal.unwrap_or(SignalType::Kill)),
            }
        };

        // Keep the original handles through escalation: a child may exit and
        // stop being tracked while its descendants still hold its output pipes
        let mut force = children;

        if let Some(repeated) = repeated {
            let running = self.children();

            // Once every child from this shutdown has exited, this is a new
            // shutdown request rather than an escalation of this one
            if !force
                .iter()
                .any(|child| running.iter().any(|other| other.same_child(child)))
            {
                for child in &force {
                    child.stop_output();
                }

                return Some(repeated);
            }

            debug!(
                signal = ?repeated,
                "Received another signal during shutdown; force killing children"
            );

            // The repeated signal applies to the entire set, including
            // children that were tracked after this shutdown began
            for child in running {
                if !force.iter().any(|other| other.same_child(&child)) {
                    force.push(child);
                }
            }
        }

        self.kill_children(&force).await;

        for child in &force {
            child.stop_output();
        }

        waiting.join_all().await;

        None
    }

    async fn kill_children(&self, children: &[SharedChild]) {
        debug!(
            pids = ?children.iter().map(SharedChild::id).collect::<Vec<_>>(),
            "Wait threshold exhausted, killing {} running child processes",
            children.len()
        );

        let mut killing = JoinSet::new();

        for child in children.iter().cloned() {
            killing.spawn(async move {
                let pid = child.id();

                debug!(pid, "Killing child process");

                if let Err(error) = child.kill_with_signal(SignalType::Kill).await {
                    warn!(
                        pid,
                        error = error.to_string(),
                        "Failed to kill child process"
                    );
                }

                child
            });
        }

        // Only remove the children that belonged to this shutdown request,
        // as others may have been tracked while it waited
        for child in killing.join_all().await {
            self.inner.remove(&child);
        }
    }
}

impl SetInner {
    /// Stop tracking this exact child, if it's still tracked. Another child
    /// that was given the same pid since is left alone.
    pub(crate) fn remove(&self, child: &SharedChild) {
        let removed = {
            let mut state = self.state.lock().unwrap();

            match state.children.get(&child.id()) {
                Some(tracked) if tracked.same_child(child) => state.children.remove(&child.id()),
                _ => None,
            }
        };

        if removed.is_some() {
            self.idle.notify_waiters();
        }
    }
}

impl fmt::Debug for ProcessSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProcessSet")
            .field("grace", &self.grace())
            .field("children", &self.inner.state.lock().unwrap().children.len())
            .finish()
    }
}

// Marks the shutdown as finished when it ends, or is cancelled
struct ShutdownGuard<'a> {
    inner: &'a SetInner,
    requests: mpsc::UnboundedSender<SignalType>,
}

impl Drop for ShutdownGuard<'_> {
    fn drop(&mut self) {
        let mut state = self.inner.state.lock().unwrap();

        if state
            .shutdown
            .as_ref()
            .is_some_and(|current| current.same_channel(&self.requests))
        {
            state.shutdown = None;
        }
    }
}
