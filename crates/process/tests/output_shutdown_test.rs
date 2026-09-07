#![cfg(unix)]

use starbase_process::{ChildExit, ProcessRegistry, SharedChild, SignalType};
use std::future::{Future, poll_fn};
use std::process::Stdio;
use std::task::Poll;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::timeout;

// The shell exits, but its background child keeps both output pipes open.
fn shell() -> Child {
    Command::new("sh")
        .args(["-c", "sleep 30 & echo $! >&2; printf ready; exit 7"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap()
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

async fn assert_capture_stops(registry: Option<&ProcessRegistry>, explicit_signal: bool) {
    let child = match registry {
        Some(registry) => registry.add_running(shell()).await,
        None => SharedChild::new(shell()),
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
    poll_fn(|cx| {
        assert!(capture.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;

    if let Some(registry) = registry {
        registry.terminate_running();
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
async fn registry_threshold_stops_capture_after_parent_was_removed() {
    let registry = ProcessRegistry::new(50);
    assert_capture_stops(Some(&registry), false).await;
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
    let registry = ProcessRegistry::instance();
    let found = timeout(Duration::from_secs(3), async {
        loop {
            if let Ok(pids) = std::fs::read_to_string(&marker) {
                let pids = pids.split_whitespace().collect::<Vec<_>>();

                if pids.len() == 2
                    && let Some(child) = registry.get_running_by_pid(pids[0].parse().unwrap()).await
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
    assert!(registry.get_running_by_pid(child.id()).await.is_none());
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
