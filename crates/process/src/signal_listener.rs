use crate::signal::{SignalType, listen};
use std::sync::{LazyLock, Once, OnceLock, mpsc};
use std::thread;
use tokio::sync::broadcast::{self, Receiver, Sender};
use tracing::warn;

static SENDER: LazyLock<Sender<SignalType>> = LazyLock::new(|| broadcast::channel(10).0);
static RECEIVED: OnceLock<SignalType> = OnceLock::new();
static STARTED: Once = Once::new();

/// Listens for OS termination signals on behalf of the whole process
/// (`SIGHUP`, `SIGINT`, `SIGQUIT`, and `SIGTERM` on Unix; `CTRL-C`,
/// `CTRL-BREAK`, `CTRL-CLOSE`, and `CTRL-SHUTDOWN` on Windows) and
/// broadcasts each one. It runs on a thread of its own, so it keeps
/// listening after the runtime that started it shuts down.
pub struct SignalListener;

impl SignalListener {
    /// Subscribe to OS termination signals, starting the listener if needed.
    /// Once this returns, those signals are broadcast instead of taking
    /// their default action of exiting the process.
    pub fn subscribe() -> Receiver<SignalType> {
        let receiver = SENDER.subscribe();

        start();

        receiver
    }

    /// Return the first OS termination signal received, if any. Unlike a
    /// new receiver, this also sees a signal that arrived before the call.
    pub fn received() -> Option<SignalType> {
        RECEIVED.get().copied()
    }
}

fn start() {
    STARTED.call_once(|| {
        let (ready_sender, ready_receiver) = mpsc::channel();

        let spawned = thread::Builder::new()
            .name("starbase-signals".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        warn!(%error, "Failed to start the signal listener");
                        return;
                    }
                };

                runtime.block_on(listen(
                    || {
                        let _ = ready_sender.send(());
                    },
                    |signal| {
                        let _ = RECEIVED.set(signal);

                        // Having no subscribers right now is fine, as
                        // later subscribers still receive later signals
                        let _ = SENDER.send(signal);

                        true
                    },
                ));
            });

        match spawned {
            // Wait until the handlers are installed, or the thread gave up
            Ok(_) => {
                let _ = ready_receiver.recv();
            }
            Err(error) => {
                warn!(%error, "Failed to start the signal listener");
            }
        }
    });
}
