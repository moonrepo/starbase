use crate::shared_child::SharedChild;
use crate::signal::SignalType;
use crate::signal_listener;
use parking_lot::Mutex;
use process_wrap::tokio::{ChildWrapper, CommandWrap};
use std::fmt;
use std::io;
use std::mem;
use std::sync::{Arc, OnceLock, Weak};
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

    // Held while a shutdown of this set starts, so that requests arriving
    // at the same time see each other
    starting: Mutex<()>,

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

    // Children of a zero grace shutdown that exited with their output
    // readers left running, for the next request to stop
    unstopped: Vec<SharedChild>,
}

// One shutdown of a set, covering every child in its subtree at the time
struct Round {
    children: Vec<Tracked>,

    // Woken when another request takes over this shutdown
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

    /// Set the root's grace period, which OS termination signals use for
    /// every child. Sets nested afterwards inherit it. A zero duration
    /// waits indefinitely instead of force killing.
    pub fn configure_root(grace: Duration) {
        *Self::root().inner.grace.lock() = grace;
    }

    /// Create a set nested under this one, with the same grace period. It's
    /// shut down along with its parent, or on its own.
    pub fn nested(&self) -> ProcessSet {
        self.nested_with_grace(self.grace())
    }

    /// Create a set nested under this one, with its own grace period for
    /// shutdowns of the set itself. A shutdown of an enclosing set uses
    /// that set's grace period instead. A zero duration waits indefinitely
    /// instead of force killing.
    pub fn nested_with_grace(&self, grace: Duration) -> ProcessSet {
        let set = Self::create(grace, Arc::clone(&self.inner.idle));
        let mut state = self.inner.state.lock();

        state.nested.retain(|nested| nested.strong_count() > 0);
        state.nested.push(Arc::downgrade(&set.inner));

        set
    }

    fn create(grace: Duration, idle: Arc<Notify>) -> ProcessSet {
        ProcessSet {
            inner: Arc::new(SetInner {
                grace: Mutex::new(grace),
                idle,
                starting: Mutex::new(()),
                state: Mutex::new(SetState::default()),
            }),
        }
    }

    /// Return how long children get to exit after a shutdown of this set
    /// signals them, before they're force killed. Zero waits indefinitely
    /// instead.
    pub fn grace(&self) -> Duration {
        *self.inner.grace.lock()
    }

    /// Spawn the command and track the child in this set. The returned
    /// handle owns the child: dropping it while the child still runs kills
    /// the child, and dropping it after the child exits stops tracking it.
    /// The set stays alive while the handle does.
    pub fn spawn(&self, command: &mut TokioCommand) -> io::Result<SharedChild> {
        self.track(Box::new(command.spawn()?), false)
    }

    /// Like [`Self::spawn`], but the child and every process it starts run
    /// in a process group (Unix) or job object (Windows) of their own.
    /// Signals and kills then reach that whole tree, including after the
    /// child itself has exited, while it's still tracked. The terminal no
    /// longer signals it directly, so the [`SignalListener`] forwards each
    /// signal instead. Processes the child leaves behind when it exits on
    /// its own keep running.
    ///
    /// On Unix, a child outside the terminal's foreground process group is
    /// stopped when it reads from the terminal, so don't contain a child
    /// that inherits stdin.
    ///
    /// [`SignalListener`]: crate::SignalListener
    pub fn spawn_contained(&self, command: &mut TokioCommand) -> io::Result<SharedChild> {
        let mut wrap = CommandWrap::from(mem::replace(command, TokioCommand::new("")));

        #[cfg(unix)]
        wrap.wrap(process_wrap::tokio::ProcessGroup::leader());

        #[cfg(windows)]
        wrap.wrap(process_wrap::tokio::JobObject);

        let child = wrap.spawn();

        *command = wrap.into_command();

        self.track(child?, true)
    }

    fn track(&self, child: Box<dyn ChildWrapper>, contained: bool) -> io::Result<SharedChild> {
        let owner = SharedChild::new_owned(child, contained, Arc::clone(&self.inner));

        self.inner.state.lock().children.push(owner.clone());

        Ok(owner)
    }

    /// Look up a child tracked in this set by pid. Nested sets aren't searched.
    pub fn get(&self, pid: u32) -> Option<SharedChild> {
        self.inner
            .state
            .lock()
            .children
            .iter()
            .rev()
            .find(|child| child.id() == pid)
            .cloned()
    }

    /// Shut down every child in this set and its nested sets. Each one is
    /// sent `signal`, unless another shutdown is already waiting on it (a
    /// force kill is always sent). After this set's grace period, the
    /// children of this shutdown are force killed, which also stops their
    /// output readers. With a zero grace period, it waits for them to exit
    /// instead, and leaves their output readers for the next request to
    /// stop.
    ///
    /// Calling this while a shutdown of this set, or of a set nested under
    /// it, is waiting out its grace period force kills everything still
    /// running under that set, including children tracked since it began.
    /// Once every child of that shutdown has exited, it starts a new
    /// shutdown instead.
    ///
    /// This returns once the grace period has passed, or once the children
    /// it force killed have exited. Use [`Self::wait_idle`] to wait for the
    /// children themselves.
    pub async fn shutdown(&self, signal: SignalType) {
        self.begin_shutdown(signal, false).finish().await;
    }

    /// Signal the children, or force kill them for a repeated request, and
    /// return the rest of the shutdown to await. Nothing here waits, so the
    /// signal listener can do this before it tells subscribers. `from_os`
    /// is set for a signal the OS sent us.
    pub(crate) fn begin_shutdown(&self, signal: SignalType, from_os: bool) -> Shutdown {
        let _starting = self.inner.starting.lock();
        let sets = self.subtree();
        let mut killed = vec![];

        for (index, set) in sets.iter().enumerate() {
            let unstopped = mem::take(&mut set.inner.state.lock().unstopped);

            for child in unstopped {
                child.stop_output();
            }

            let Some(round) = set.current_shutdown() else {
                continue;
            };

            if round.has_running() {
                killed.extend(set.force_kill(&round));

                // That covered everything under this set, so let the
                // shutdowns of nested sets finish rather than wait
                if index == 0 {
                    for nested in &sets[1..] {
                        if let Some(round) = nested.current_shutdown() {
                            round.supersede();
                        }
                    }

                    return Shutdown {
                        killed,
                        round: None,
                    };
                }
            } else {
                // Every child of that shutdown has exited, so this request
                // starts a new shutdown rather than escalating that one
                round.supersede();
                set.clear_shutdown(&round);
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

            // A child that another shutdown is still waiting on, such as one
            // of an enclosing set, isn't signalled again, as many programs
            // treat a second interrupt as a request to quit at once
            let first = child.join_shutdown();

            // Neither is an uncontained child, which shares our process
            // group, for the signals a terminal sends that whole group
            let delivered = from_os
                && cfg!(unix)
                && !child.is_contained()
                && matches!(
                    signal,
                    SignalType::Hangup | SignalType::Interrupt | SignalType::Quit
                );

            if ((first && !delivered) || matches!(signal, SignalType::Kill))
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

        self.inner.state.lock().shutdown = Some(Arc::clone(&round));

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
                .all(|set| set.inner.state.lock().children.is_empty())
            {
                return;
            }

            notified.await;
        }
    }

    fn current_shutdown(&self) -> Option<Arc<Round>> {
        self.inner.state.lock().shutdown.clone()
    }

    fn clear_shutdown(&self, round: &Arc<Round>) {
        let mut state = self.inner.state.lock();

        if state
            .shutdown
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, round))
        {
            state.shutdown = None;
        }
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
        let mut state = self.inner.state.lock();

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
                let children = set.inner.state.lock().children.clone();

                children.into_iter().map(move |child| Tracked {
                    set: Arc::clone(&set.inner),
                    child,
                })
            })
            .collect()
    }
}

/// The part of a shutdown that waits, returned by
/// [`ProcessSet::begin_shutdown`]. Dropping it unfinished cancels the
/// shutdown.
pub(crate) struct Shutdown {
    killed: Vec<Tracked>,
    round: Option<ActiveRound>,
}

impl Shutdown {
    pub(crate) async fn finish(self) {
        let Shutdown { killed, round } = self;
        let mut killing = JoinSet::new();

        for tracked in killed {
            killing.spawn(wait_after_kill_and_remove(tracked));
        }

        if let Some(round) = round {
            round.run().await;
        }

        killing.join_all().await;
    }
}

enum End {
    Superseded,
    Exited,
    TimedOut,
}

struct ActiveRound {
    set: ProcessSet,
    round: Arc<Round>,
    signal: SignalType,
    grace: Duration,
}

impl ActiveRound {
    async fn run(self) {
        let mut waiting = JoinSet::new();

        for tracked in self.round.children.iter().cloned() {
            waiting.spawn(wait_and_remove(tracked));
        }

        let end = if matches!(self.signal, SignalType::Kill) {
            End::TimedOut
        } else if self.grace.is_zero() {
            tokio::select! {
                _ = self.round.superseded.notified() => End::Superseded,
                _ = async { while waiting.join_next().await.is_some() {} } => End::Exited,
            }
        } else {
            tokio::select! {
                _ = self.round.superseded.notified() => End::Superseded,
                _ = sleep(self.grace) => End::TimedOut,
            }
        };

        match end {
            // Whatever took over this shutdown has dealt with its children,
            // and the waits still running here are dropped
            End::Superseded => {}

            // With no limit on the grace period, readers that descendants
            // keep open stay running until another request stops them
            End::Exited => {
                self.set.inner.state.lock().unstopped.extend(
                    self.round
                        .children
                        .iter()
                        .map(|tracked| tracked.child.clone()),
                );
            }

            // Children that already exited are only stopped from holding up
            // their output readers, which descendants may be keeping open
            End::TimedOut => {
                debug!(
                    pids = ?self.round.children.iter().map(|tracked| tracked.child.id()).collect::<Vec<_>>(),
                    "Wait threshold exhausted, killing {} running child processes",
                    self.round.children.len()
                );

                drop(waiting);

                let mut killing = JoinSet::new();

                for tracked in self.round.children.iter().cloned() {
                    kill(&tracked.child);
                    killing.spawn(wait_after_kill_and_remove(tracked));
                }

                killing.join_all().await;
            }
        }
    }
}

// Runs whether the round finished, was cancelled, or never ran
impl Drop for ActiveRound {
    fn drop(&mut self) {
        for tracked in &self.round.children {
            tracked.child.leave_shutdown();
        }

        self.set.clear_shutdown(&self.round);
    }
}

impl Round {
    // Whether any child of this shutdown is still tracked, which it stops
    // being once it has exited, or once it's released from its set
    fn has_running(&self) -> bool {
        self.children
            .iter()
            .any(|tracked| tracked.set.tracks(&tracked.child))
    }

    // End this shutdown early, stopping its children's output readers
    fn supersede(&self) {
        for tracked in &self.children {
            tracked.child.stop_output();
        }

        self.superseded.notify_one();
    }
}

impl SetInner {
    fn tracks(&self, child: &SharedChild) -> bool {
        self.state
            .lock()
            .children
            .iter()
            .any(|tracked| tracked.same_child(child))
    }

    /// Stop tracking this exact child, if it's still tracked. Another child
    /// that was given the same pid since is left alone.
    pub(crate) fn remove(&self, child: &SharedChild) {
        let removed = {
            let mut state = self.state.lock();

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
            .field("children", &self.inner.state.lock().children.len())
            .finish()
    }
}

async fn wait_and_remove(tracked: Tracked) {
    let _ = tracked.child.wait().await;

    tracked.set.remove(&tracked.child);
}

async fn wait_after_kill_and_remove(tracked: Tracked) {
    tracked.child.wait_after_kill().await;
    tracked.set.remove(&tracked.child);
}

fn kill(child: &SharedChild) {
    if let Err(error) = child.send_signal(SignalType::Kill) {
        warn!(pid = child.id(), %error, "Failed to kill child process");
    }
}
