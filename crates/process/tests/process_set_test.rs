#![cfg(unix)]

use starbase_console::{Console, EmptyReporter};
use starbase_process::{ChildExit, Command, ProcessSet, SharedChild, SignalType};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command as TokioCommand;
use tokio::time::timeout;

// Every test works in a set nested under the root, so tests running in
// parallel never shut down each other's children
fn create_set() -> ProcessSet {
    ProcessSet::root().nested()
}

fn spawn_sleep(set: &ProcessSet) -> SharedChild {
    set.spawn(TokioCommand::new("sleep").arg("30")).unwrap()
}

// Shutdowns wait out the grace period, so run them in the background
fn shutdown_in_background(set: &ProcessSet) {
    let set = set.clone();

    tokio::spawn(async move { set.shutdown(SignalType::Terminate).await });
}

async fn wait_for_exit(child: &SharedChild) -> ChildExit {
    timeout(Duration::from_secs(1), child.wait())
        .await
        .expect("child did not exit")
        .unwrap()
}

mod process_set {
    use super::*;

    #[tokio::test]
    async fn shutting_down_a_set_shuts_down_its_nested_sets() {
        let parent = create_set();
        let nested = parent.nested();
        let child = spawn_sleep(&nested);

        shutdown_in_background(&parent);

        assert_eq!(wait_for_exit(&child).await, ChildExit::Terminated(15));
    }

    #[tokio::test]
    async fn shutting_down_a_nested_set_leaves_its_parent_alone() {
        let parent = create_set();
        let nested = parent.nested();
        let parent_child = spawn_sleep(&parent);
        let nested_child = spawn_sleep(&nested);

        shutdown_in_background(&nested);

        assert_eq!(
            wait_for_exit(&nested_child).await,
            ChildExit::Terminated(15)
        );
        assert!(
            timeout(Duration::from_millis(100), parent_child.wait())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn dropping_the_owner_kills_a_running_child() {
        let set = create_set();
        let owner = spawn_sleep(&set);
        let child = owner.clone();

        drop(owner);

        assert_eq!(wait_for_exit(&child).await, ChildExit::Killed);
        timeout(Duration::from_secs(1), set.wait_idle())
            .await
            .expect("cancelled child remained tracked");
    }

    #[tokio::test]
    async fn dropping_the_owner_after_exit_only_stops_tracking() {
        let set = create_set();
        let owner = set.spawn(&mut TokioCommand::new("true")).unwrap();

        assert!(matches!(
            wait_for_exit(&owner).await,
            ChildExit::Completed(status) if status.success()
        ));

        drop(owner);

        timeout(Duration::from_secs(1), set.wait_idle())
            .await
            .expect("exited child remained tracked");
    }

    #[tokio::test]
    async fn waiting_for_idle_includes_nested_sets() {
        let parent = create_set();
        let nested = parent.nested();
        let owner = spawn_sleep(&nested);

        assert!(
            timeout(Duration::from_millis(100), parent.wait_idle())
                .await
                .is_err()
        );

        drop(owner);

        timeout(Duration::from_secs(1), parent.wait_idle())
            .await
            .expect("parent did not become idle");
    }

    #[tokio::test]
    async fn commands_run_in_their_own_set() {
        let set = create_set();
        let mut command = Command::<EmptyReporter>::new("sleep");
        command.arg("30");
        command.no_shell();
        command.set_console(Console::new_testing());
        command.set_error_on_nonzero(false);
        command.set_process_set(set.clone());

        let task = tokio::spawn(async move { command.exec_capture_output().await });

        // The set stops being idle once the command's process is tracked.
        // Sleep between checks, as an idle set returns without yielding.
        timeout(Duration::from_secs(1), async {
            while timeout(Duration::from_millis(50), set.wait_idle())
                .await
                .is_ok()
            {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("command was not tracked in its set");

        shutdown_in_background(&set);

        let output = timeout(Duration::from_secs(1), task)
            .await
            .expect("command did not exit")
            .unwrap()
            .unwrap();

        assert_eq!(output.exit, ChildExit::Terminated(15));
    }

    #[tokio::test]
    async fn finds_tracked_children_by_pid() {
        let set = create_set();
        let child = spawn_sleep(&set);
        let pid = child.id();

        assert!(set.get(pid).is_some());
        assert!(set.get(0).is_none());

        drop(child);

        assert!(set.get(pid).is_none());
    }

    #[tokio::test]
    async fn shutdown_stops_tracking_exited_children() {
        let set = create_set();
        let child = spawn_sleep(&set);
        let pid = child.id();

        shutdown_in_background(&set);

        assert_eq!(wait_for_exit(&child).await, ChildExit::Terminated(15));
        timeout(Duration::from_secs(1), set.wait_idle())
            .await
            .expect("exited child remained tracked");
        assert!(set.get(pid).is_none());
    }

    #[tokio::test]
    async fn a_shutdown_with_no_children_does_not_stop_later_ones() {
        let set = create_set();

        set.shutdown(SignalType::Terminate).await;

        let child = spawn_sleep(&set);

        shutdown_in_background(&set);

        assert_eq!(wait_for_exit(&child).await, ChildExit::Terminated(15));
    }

    #[tokio::test]
    async fn later_shutdowns_reach_children_tracked_after_earlier_ones() {
        let set = create_set();
        let first = spawn_sleep(&set);

        shutdown_in_background(&set);

        assert_eq!(wait_for_exit(&first).await, ChildExit::Terminated(15));
        timeout(Duration::from_secs(1), set.wait_idle())
            .await
            .expect("exited child remained tracked");

        let second = spawn_sleep(&set);

        shutdown_in_background(&set);

        assert_eq!(wait_for_exit(&second).await, ChildExit::Terminated(15));
    }

    #[tokio::test]
    async fn a_zero_grace_period_still_signals_before_waiting() {
        let set = ProcessSet::root().nested_with_grace(Duration::ZERO);
        let child = spawn_sleep(&set);

        shutdown_in_background(&set);

        assert_eq!(wait_for_exit(&child).await, ChildExit::Terminated(15));
        timeout(Duration::from_secs(1), set.wait_idle())
            .await
            .expect("exited child remained tracked");
    }

    #[tokio::test]
    async fn a_second_shutdown_force_kills_without_waiting_for_the_grace_period() {
        let set = ProcessSet::root().nested_with_grace(Duration::from_secs(5));
        let child = set
            .spawn(
                TokioCommand::new("sh")
                    .args(["-c", "trap '' TERM; echo ready; exec sleep 30"])
                    .stdout(Stdio::piped()),
            )
            .unwrap();

        let mut ready = String::new();
        BufReader::new(child.take_stdout().await.unwrap())
            .read_line(&mut ready)
            .await
            .unwrap();
        assert_eq!(ready, "ready\n");

        shutdown_in_background(&set);
        shutdown_in_background(&set);

        assert_eq!(wait_for_exit(&child).await, ChildExit::Killed);
    }

    #[tokio::test]
    async fn waiting_for_idle_returns_at_once_when_empty() {
        timeout(Duration::from_secs(1), create_set().wait_idle())
            .await
            .expect("empty set was not idle");
    }
}
