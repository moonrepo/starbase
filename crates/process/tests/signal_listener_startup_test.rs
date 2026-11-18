#![cfg(unix)]

// The listener starts once per process, so this lives in its own binary to
// be sure the runtime below is the one that starts it, and that no other
// test sends signals to this process.

use starbase_process::{SignalListener, SignalType, kill};
use std::time::Duration;
use tokio::sync::broadcast::Receiver;
use tokio::time::timeout;

fn new_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn send_and_receive(receiver: &mut Receiver<SignalType>, signal: SignalType) -> SignalType {
    kill(std::process::id(), signal).unwrap();

    new_runtime()
        .block_on(async { timeout(Duration::from_secs(5), receiver.recv()).await })
        .expect("signal was not broadcast")
        .unwrap()
}

#[test]
fn outlives_the_runtime_that_started_it_and_keeps_the_first_signal() {
    // Start the listener from a runtime that then shuts down
    drop(new_runtime().block_on(async { SignalListener::subscribe() }));

    assert!(SignalListener::received().is_none());

    let mut receiver = SignalListener::subscribe();

    assert!(matches!(
        send_and_receive(&mut receiver, SignalType::Hangup),
        SignalType::Hangup
    ));
    assert!(matches!(
        send_and_receive(&mut receiver, SignalType::Terminate),
        SignalType::Terminate
    ));
    assert!(matches!(
        SignalListener::received(),
        Some(SignalType::Hangup)
    ));
}
