#![cfg(unix)]

use starbase_console::{Console, EmptyReporter};
use starbase_process::{ChildExit, Command, ProcessSet, SharedChild, SignalType};
use std::time::Duration;
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
}
