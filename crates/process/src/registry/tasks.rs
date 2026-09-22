use super::event::ProcessEvent;
use super::state::RegistryState;
use crate::signal::SignalType;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

pub(super) struct Tasks {
    pub(super) reaper: JoinHandle<()>,
    pub(super) coordinator: JoinHandle<()>,
    pub(super) signals: Option<JoinHandle<()>>,
}

/// Detect exited children and release dropped ones.
pub(super) async fn run_reaper(state: Arc<RegistryState>) {
    // Holding the lock for the life of the task hands the receiver back
    // when the task is aborted, so a restart resumes where it left off
    let mut cleanup = state.cleanup_receiver.lock().await;

    #[cfg(unix)]
    let mut sigchld = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::child()).ok();

    #[cfg(not(unix))]
    let mut sigchld = ();

    let mut interval = tokio::time::interval(state.options.reap_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    // Children may have exited while we weren't running
    state.reap_exited().await;

    loop {
        tokio::select! {
            Some(child) = cleanup.recv() => {
                // A kill waits for the child to exit, so don't block the
                // reaper on it
                tokio::spawn(Arc::clone(&state).cleanup_dropped(child));
            },
            _ = state.reap.notified() => state.reap_exited().await,
            _ = interval.tick() => state.reap_exited().await,
            _ = wait_for_sigchld(&mut sigchld) => state.reap_exited().await,
        }
    }
}

#[cfg(unix)]
async fn wait_for_sigchld(signal: &mut Option<tokio::signal::unix::Signal>) {
    match signal {
        Some(signal) => {
            signal.recv().await;
        }
        None => std::future::pending().await,
    }
}

#[cfg(not(unix))]
async fn wait_for_sigchld(_signal: &mut ()) {
    std::future::pending().await
}

/// Turn every received signal into a shutdown of the running children.
pub(super) async fn run_shutdown_coordinator(
    state: Arc<RegistryState>,
    mut receiver: broadcast::Receiver<SignalType>,
) {
    let mut pending = None;

    loop {
        let signal = match pending.take() {
            Some(signal) => signal,
            None => match receiver.recv().await {
                Ok(signal) => signal,
                Err(broadcast::error::RecvError::Lagged(_)) => SignalType::Kill,
                Err(broadcast::error::RecvError::Closed) => break,
            },
        };

        state.emit(ProcessEvent::Signal(signal));

        pending = state.perform_shutdown(signal, Some(&mut receiver)).await;
    }
}
