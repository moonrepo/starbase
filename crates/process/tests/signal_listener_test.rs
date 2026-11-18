#![cfg(unix)]

// Signals are sent to this test process itself, which shuts down every
// process set in the same binary, so this is the binary's only test.

use starbase_process::{ChildExit, ProcessSet, SignalListener, SignalType, kill};
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

fn sleep_command() -> Command {
    let mut command = Command::new("sleep");
    command.arg("30");
    command
}

#[tokio::test]
async fn shuts_down_the_root_and_broadcasts_signals() {
    let mut receiver = SignalListener::subscribe();
    let set = ProcessSet::root().nested();
    let contained = set.spawn_contained(sleep_command()).unwrap();
    let uncontained = set.spawn(sleep_command()).unwrap();

    // Handlers are installed once subscribing returns, so this signal is
    // broadcast instead of exiting the test process
    kill(std::process::id(), SignalType::Hangup).unwrap();

    let received = timeout(Duration::from_secs(5), receiver.recv())
        .await
        .expect("SIGHUP was not broadcast")
        .unwrap();

    assert!(matches!(received, SignalType::Hangup));
    assert_eq!(
        timeout(Duration::from_secs(5), contained.wait())
            .await
            .expect("contained child was not shutdown")
            .unwrap(),
        ChildExit::Terminated(SignalType::Hangup.get_code())
    );

    // A child in our process group would have received a terminal's
    // SIGHUP directly, so it isn't sent again. It's force killed once
    // the root's grace period ends.
    assert!(
        timeout(Duration::from_millis(500), uncontained.wait())
            .await
            .is_err()
    );
    assert_eq!(
        timeout(Duration::from_secs(5), uncontained.wait())
            .await
            .expect("uncontained child was not force killed")
            .unwrap(),
        ChildExit::Killed
    );
}
