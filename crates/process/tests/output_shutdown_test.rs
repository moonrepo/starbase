#![cfg(unix)]

use starbase_process::{ChildExit, ProcessSet, SharedChild, SignalType};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;

// The shell exits, but its background child keeps both output pipes open.
fn shell() -> Command {
    let mut command = Command::new("sh");
    command
        .args(["-c", "sleep 30 & echo $! >&2; printf ready; exit 7"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    command
}

struct Descendant(u32);
impl Drop for Descendant {
    fn drop(&mut self) {
        // Clean up the test's own sleeper, which is still alive after its
        // parent exits. Closing capture pipes does not terminate descendants.
        let _ = starbase_process::kill(self.0, SignalType::Kill);
    }
}

async fn descendant(child: &SharedChild) -> Descendant {
    let mut pid = String::new();
    timeout(
        Duration::from_secs(3),
        BufReader::new(child.take_stderr().await.unwrap()).read_line(&mut pid),
    )
    .await
    .unwrap()
    .unwrap();
    Descendant(pid.trim().parse().unwrap())
}

async fn assert_capture_stops(set: Option<&ProcessSet>, explicit_signal: bool) {
    let child = match set {
        Some(set) => set.spawn(shell()).unwrap(),
        None => SharedChild::new(shell().spawn().unwrap()),
    };
    let _descendant = descendant(&child).await;
    let exit = timeout(Duration::from_secs(3), child.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(exit, ChildExit::Completed(status) if status.code() == Some(7)));

    let capture = child.wait_with_output();
    tokio::pin!(capture);
    // Consume the already-written output, then block on the descendant's pipe.
    // One poll isn't enough, as the pipe only reads once the reactor has
    // reported it readable, and that may not have happened yet.
    assert!(
        timeout(Duration::from_millis(50), capture.as_mut())
            .await
            .is_err()
    );

    if let Some(set) = set {
        // The shutdown waits out the grace period, so run it in the background
        let set = set.clone();

        tokio::spawn(async move { set.shutdown(SignalType::Terminate).await });
    } else if explicit_signal {
        child.kill_with_signal(SignalType::Kill).await.unwrap();
    } else {
        child.kill().await.unwrap();
    }

    let output = timeout(Duration::from_secs(1), capture)
        .await
        .expect("capture waited on a descendant after forced shutdown")
        .unwrap();
    assert_eq!(output.exit, exit);
    assert_eq!(output.stdout.as_ref(), b"ready");
}

#[tokio::test]
async fn kill_stops_capture_even_after_parent_was_reaped() {
    assert_capture_stops(None, false).await;
}

#[tokio::test]
async fn kill_signal_stops_capture_even_after_parent_was_reaped() {
    assert_capture_stops(None, true).await;
}

#[tokio::test]
async fn shutdown_grace_period_stops_capture_after_parent_exited() {
    let set = ProcessSet::root().nested_with_grace(Duration::from_millis(50));
    assert_capture_stops(Some(&set), false).await;
}

async fn assert_streaming_capture_stops(continuous: bool) {
    use starbase_console::{Console, EmptyReporter};

    let marker = std::env::temp_dir().join(format!(
        "starbase-output-shutdown-{}-{continuous}",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&marker);
    let mut command = starbase_process::Command::<EmptyReporter>::new("sh");
    command.no_shell().args([
        "-c",
        &format!(
            "sleep 30 & echo \"$$ $!\" > '{}'; printf 'ready\\n'; exit 7",
            marker.display()
        ),
    ]);
    command.set_console(Console::new_testing());
    command.set_error_on_nonzero(false);
    let task = tokio::spawn(async move {
        if continuous {
            command.exec_capture_continuous_output().await
        } else {
            command.exec_stream_and_capture_output().await
        }
    });
    let set = ProcessSet::root();
    let found = timeout(Duration::from_secs(3), async {
        loop {
            if let Ok(pids) = std::fs::read_to_string(&marker) {
                let pids = pids.split_whitespace().collect::<Vec<_>>();

                if pids.len() == 2
                    && let Some(child) = set.get(pids[0].parse().unwrap())
                {
                    break (child, Descendant(pids[1].parse().unwrap()));
                }
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
    if found.is_err() {
        task.abort();
    }
    let (child, _descendant) = found.expect("child disappeared before capture completed");
    let exit = timeout(Duration::from_secs(3), child.wait())
        .await
        .unwrap()
        .unwrap();
    child.kill_with_signal(SignalType::Kill).await.unwrap();
    let mut task = task;
    let result = timeout(Duration::from_secs(1), &mut task).await;
    if result.is_err() {
        task.abort();
    }
    let output = result
        .expect("streaming capture waited on inherited pipes")
        .unwrap()
        .unwrap();
    assert_eq!(output.exit, exit);
    assert!(set.get(child.id()).is_none());
    std::fs::remove_file(marker).unwrap();
}

#[tokio::test]
async fn force_kill_stops_stream_and_capture_readers() {
    assert_streaming_capture_stops(false).await;
}

#[tokio::test]
async fn force_kill_stops_continuous_capture_readers() {
    assert_streaming_capture_stops(true).await;
}

// Spawn the shell into `set`, wait for it to exit, and return a capture that
// is blocked on the pipes its background child still holds
async fn blocked_capture(
    set: &ProcessSet,
) -> (
    SharedChild,
    Descendant,
    std::pin::Pin<
        Box<impl std::future::Future<Output = std::io::Result<starbase_process::Output>>>,
    >,
) {
    let child = set.spawn(shell()).unwrap();
    let descendant = descendant(&child).await;

    timeout(Duration::from_secs(3), child.wait())
        .await
        .unwrap()
        .unwrap();

    let mut capture = Box::pin({
        let child = child.clone();

        async move { child.wait_with_output().await }
    });

    assert!(
        timeout(Duration::from_millis(50), capture.as_mut())
            .await
            .is_err()
    );

    (child, descendant, capture)
}

#[tokio::test]
async fn a_later_request_stops_capture_after_a_zero_grace_shutdown() {
    let set = ProcessSet::root().nested_with_grace(Duration::ZERO);
    let (_child, _descendant, mut capture) = blocked_capture(&set).await;

    // With an unlimited grace period, nothing is force killed or stopped
    set.shutdown(SignalType::Terminate).await;

    assert!(
        timeout(Duration::from_millis(50), capture.as_mut())
            .await
            .is_err()
    );

    // So the next request stops the capture
    set.shutdown(SignalType::Terminate).await;

    let output = timeout(Duration::from_secs(1), capture)
        .await
        .expect("capture waited on a descendant after a second request")
        .unwrap();

    assert_eq!(output.stdout.as_ref(), b"ready");
}

#[tokio::test]
async fn a_parent_shutdown_stops_capture_for_a_nested_shutdowns_exited_child() {
    let parent = ProcessSet::root().nested_with_grace(Duration::from_secs(10));
    let nested = parent.nested_with_grace(Duration::from_secs(10));
    let (_child, _descendant, capture) = blocked_capture(&nested).await;

    // The nested shutdown sees its child exit, and waits out its grace period
    tokio::spawn({
        let nested = nested.clone();

        async move { nested.shutdown(SignalType::Terminate).await }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;

    // A shutdown of the parent takes over, which stops the capture
    tokio::spawn({
        let parent = parent.clone();

        async move { parent.shutdown(SignalType::Terminate).await }
    });

    timeout(Duration::from_secs(1), capture)
        .await
        .expect("capture waited out the nested shutdown's grace period")
        .unwrap();
}
