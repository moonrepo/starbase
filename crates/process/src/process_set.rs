use crate::shared_child::SharedChild;
use crate::signal::SignalType;
use crate::signal_listener;
use std::fmt;
use std::io;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;
use tokio::process::Command as TokioCommand;
use tokio::sync::Notify;
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
    // A list rather than a map keyed by pid, so a child given the pid of
    // an exited one that is still tracked doesn't replace it
    children: Vec<SharedChild>,
    nested: Vec<Weak<SetInner>>,

    // The shutdown of this set that is waiting out its grace period
    shutdown: Option<Arc<Round>>,
}

// One shutdown of a set, covering every child in its subtree at the time
struct Round {
    children: Vec<Tracked>,

    // Woken when another request force kills the children, or starts a
    // new shutdown after they all exited
    superseded: Notify,
}

// A child, and the set tracking it
#[derive(Clone)]
struct Tracked {
    set: Arc<SetInner>,
    child: SharedChild,
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

    /// Create a set nested under this one, with its own grace period. A zero
    /// duration waits indefinitely instead of force killing.
    pub fn nested_with_grace(&self, grace: Duration) -> ProcessSet {
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
    /// The set stays alive while the handle does.
    pub fn spawn(&self, command: &mut TokioCommand) -> io::Result<SharedChild> {
        let owner = SharedChild::new_owned(command.spawn()?, Arc::clone(&self.inner));

        self.inner
            .state
            .lock()
            .unwrap()
            .children
            .push(owner.clone());

        Ok(owner)
    }

    /// Look up a child tracked in this set by pid. Nested sets aren't searched.
    pub fn get(&self, pid: u32) -> Option<SharedChild> {
        self.inner
            .state
            .lock()
            .unwrap()
            .children
            .iter()
            .rev()
            .find(|child| child.id() == pid)
            .cloned()
    }

    /// Shut down every child in this set and its nested sets. Each one is
    /// sent `signal`, unless it was already signalled (by a shutdown of an
    /// enclosing set, say). After the grace period, the children of this
    /// shutdown are force killed, which also stops their output readers.
    /// With a zero grace period, it waits for them to exit instead.
    ///
    /// Calling this while a shutdown of this set, or of a set nested under
    /// it, is waiting out its grace period force kills everything still
    /// running under that set, including children tracked since it began.
    /// Once every child of that shutdown has exited, it starts a new
    /// shutdown instead.
    ///
    /// This returns once the grace period has passed, or sooner when force
    /// killed. Use [`Self::wait_idle`] to wait for the children themselves.
    pub async fn shutdown(&self, signal: SignalType) {
        self.begin_shutdown(signal).finish().await;
    }

    /// Signal the children, or force kill them for a repeated request, and
    /// return the rest of the shutdown to await. Nothing here waits, so the
    /// signal listener can do this before it tells subscribers.
    pub(crate) fn begin_shutdown(&self, signal: SignalType) -> Shutdown {
        let mut killed = vec![];

        if let Some(round) = self.current_shutdown() {
            if round.has_running() {
                killed.extend(self.force_kill(&round));

                // That killed the children of nested shutdowns too, so let
                // them finish rather than wait out their grace
                for set in self.subtree().iter().skip(1) {
                    if let Some(round) = set.current_shutdown() {
                        round.superseded.notify_one();
                    }
                }

                return Shutdown {
                    killed,
                    round: None,
                };
            }

            // Every child of that shutdown has exited, so this request starts
            // a new shutdown rather than escalating that one
            for tracked in &round.children {
                tracked.child.stop_output();
            }

            round.superseded.notify_one();
        }

        // Escalate shutdowns of nested sets that are waiting out their grace
        for set in self.subtree().iter().skip(1) {
            if let Some(round) = set.current_shutdown()
                && round.has_running()
            {
                killed.extend(set.force_kill(&round));
            }
        }

        // Children force killed above are already taken care of
        let children = self
            .tracked()
            .into_iter()
            .filter(|tracked| {
                !killed
                    .iter()
                    .any(|other| other.child.same_child(&tracked.child))
            })
            .collect::<Vec<_>>();

        if children.is_empty() {
            return Shutdown {
                killed,
                round: None,
            };
        }

        debug!(
            signal = ?signal,
            pids = ?children.iter().map(|tracked| tracked.child.id()).collect::<Vec<_>>(),
            "Shutting down {} running child processes",
            children.len()
        );

        for tracked in &children {
            let child = &tracked.child;

            // A child already shutting down isn't signalled again, as many
            // programs treat a second interrupt as a request to quit at once
            if (matches!(signal, SignalType::Kill) || !child.was_signalled())
                && let Err(error) = child.send_signal(signal)
            {
                warn!(
                    pid = child.id(),
                    %error,
                    "Failed to shutdown child process"
                );
            }
        }

        let round = Arc::new(Round {
            children,
            superseded: Notify::new(),
        });

        self.inner.state.lock().unwrap().shutdown = Some(Arc::clone(&round));

        Shutdown {
            killed,
            round: Some(ActiveRound {
                set: self.clone(),
                round,
                signal,
                grace: self.grace(),
            }),
        }
    }

    /// Force kill every child in this set and its nested sets, without
    /// waiting for them to exit.
    pub fn kill_all(&self) {
        for tracked in self.tracked() {
            kill(&tracked.child);
        }
    }

    /// Wait until no child is tracked in this set or its nested sets. A
    /// child stops being tracked once it has exited and its owner has been
    /// dropped, or a shutdown has seen it exit. An owner dropped while its
    /// child runs stops tracking it once it has sent the kill.
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

    fn current_shutdown(&self) -> Option<Arc<Round>> {
        self.inner.state.lock().unwrap().shutdown.clone()
    }

    // Force kill the children of a waiting shutdown, and everything running
    // under this set since, then wake that shutdown so it can finish
    fn force_kill(&self, round: &Round) -> Vec<Tracked> {
        debug!("Received another signal during shutdown; force killing children");

        let mut children = round.children.clone();

        for tracked in self.tracked() {
            if !children
                .iter()
                .any(|other| other.child.same_child(&tracked.child))
            {
                children.push(tracked);
            }
        }

        for tracked in &children {
            kill(&tracked.child);
        }

        round.superseded.notify_one();

        children
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

    // Every child tracked in this set and its nested sets
    fn tracked(&self) -> Vec<Tracked> {
        self.subtree()
            .into_iter()
            .flat_map(|set| {
                let children = set.inner.state.lock().unwrap().children.clone();

                children.into_iter().map(move |child| Tracked {
                    set: Arc::clone(&set.inner),
                    child,
                })
            })
            .collect()
    }
}

/// The part of a shutdown that waits, returned by
/// [`ProcessSet::begin_shutdown`].
pub(crate) struct Shutdown {
    killed: Vec<Tracked>,
    round: Option<ActiveRound>,
}

impl Shutdown {
    pub(crate) async fn finish(self) {
        let mut waiting = JoinSet::new();

        for tracked in self.killed {
            waiting.spawn(wait_and_remove(tracked));
        }

        if let Some(round) = self.round {
            round.run().await;
        }

        waiting.join_all().await;
    }
}

struct ActiveRound {
    set: ProcessSet,
    round: Arc<Round>,
    signal: SignalType,
    grace: Duration,
}

impl ActiveRound {
    async fn run(self) {
        let _guard = RoundGuard {
            set: &self.set.inner,
            round: &self.round,
        };

        let mut waiting = JoinSet::new();

        for tracked in self.round.children.iter().cloned() {
            waiting.spawn(wait_and_remove(tracked));
        }

        let timed_out = if matches!(self.signal, SignalType::Kill) {
            true
        } else if self.grace.is_zero() {
            // Wait indefinitely for the children to exit, without force
            // killing, unless another request does
            tokio::select! {
                _ = self.round.superseded.notified() => {},
                _ = async { while waiting.join_next().await.is_some() {} } => {},
            }

            false
        } else {
            tokio::select! {
                _ = self.round.superseded.notified() => false,
                _ = sleep(self.grace) => true,
            }
        };

        // Children that already exited are only stopped from holding up their
        // output readers, which their descendants may still be keeping open
        if timed_out {
            debug!(
                pids = ?self.round.children.iter().map(|tracked| tracked.child.id()).collect::<Vec<_>>(),
                "Wait threshold exhausted, killing {} running child processes",
                self.round.children.len()
            );

            for tracked in &self.round.children {
                kill(&tracked.child);
            }
        }

        waiting.join_all().await;
    }
}

// Clears the set's shutdown when the round ends or is cancelled, unless a
// newer one has replaced it
struct RoundGuard<'a> {
    set: &'a SetInner,
    round: &'a Arc<Round>,
}

impl Drop for RoundGuard<'_> {
    fn drop(&mut self) {
        let mut state = self.set.state.lock().unwrap();

        if state
            .shutdown
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, self.round))
        {
            state.shutdown = None;
        }
    }
}

impl Round {
    // Whether any child of this shutdown is still tracked, which it stops
    // being once it has exited
    fn has_running(&self) -> bool {
        self.children
            .iter()
            .any(|tracked| tracked.set.tracks(&tracked.child))
    }
}

impl SetInner {
    fn tracks(&self, child: &SharedChild) -> bool {
        self.state
            .lock()
            .unwrap()
            .children
            .iter()
            .any(|tracked| tracked.same_child(child))
    }

    /// Stop tracking this exact child, if it's still tracked. Another child
    /// that was given the same pid since is left alone.
    pub(crate) fn remove(&self, child: &SharedChild) {
        let removed = {
            let mut state = self.state.lock().unwrap();

            state
                .children
                .iter()
                .position(|tracked| tracked.same_child(child))
                .map(|index| state.children.remove(index))
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

async fn wait_and_remove(tracked: Tracked) {
    let _ = tracked.child.wait().await;

    tracked.set.remove(&tracked.child);
}

fn kill(child: &SharedChild) {
    if let Err(error) = child.send_signal(SignalType::Kill) {
        warn!(pid = child.id(), %error, "Failed to kill child process");
    }
}
