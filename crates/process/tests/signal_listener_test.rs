#![cfg(unix)]

// Signals are sent to this test process itself, which every registry in
// the same binary would receive, so these tests live in their own binary.

use starbase_process::{ChildExit, ProcessRegistry, SignalType, kill};
use std::time::Duration;
use tokio::process::Command;
use tokio::signal::unix::{SignalKind, signal};
use tokio::time::{sleep, timeout};

mod signal_listener {
    use super::*;

    #[tokio::test]
    async fn forwards_sighup_to_running_children() {
        // Replace the default action (exit) before the registry listens
        let _hangup = signal(SignalKind::hangup()).unwrap();

        let registry = ProcessRegistry::new(5000);
        let mut receiver = registry.receive_signal();
        let child = registry
            .add_running(Command::new("sleep").arg("30").spawn().unwrap())
            .await;

        // The registry starts listening in the background, so a signal
        // sent before then is missed; keep sending until it's received
        let received = timeout(Duration::from_secs(5), async {
            loop {
                kill(std::process::id(), SignalType::Hangup).unwrap();

                tokio::select! {
                    result = receiver.recv() => break result.unwrap(),
                    _ = sleep(Duration::from_millis(250)) => {},
                }
            }
        })
        .await
        .expect("SIGHUP was not broadcast");

        assert!(matches!(received, SignalType::Hangup));
        assert_eq!(
            timeout(Duration::from_secs(5), child.wait())
                .await
                .expect("child was not shutdown")
                .unwrap(),
            ChildExit::Terminated(SignalType::Hangup.get_code())
        );
    }
}
