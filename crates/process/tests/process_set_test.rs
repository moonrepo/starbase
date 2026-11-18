#![cfg(unix)]

use starbase_console::{Console, EmptyReporter};
use starbase_process::{ChildExit, Command, ProcessSet, SharedChild, SignalType};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command as TokioCommand;
use tokio::time::{sleep, timeout};

// Every test works in a set nested under the root, so tests running in
// parallel never shut down each other's children
fn create_set() -> ProcessSet {
    ProcessSet::root().nested()
}

fn spawn_sleep(set: &ProcessSet) -> SharedChild {
    let mut command = TokioCommand::new("sleep");
    command.arg("30");

    set.spawn(command).unwrap()
}

// A shell running `script`, with its stdout piped
fn shell(script: &str) -> TokioCommand {
    let mut command = TokioCommand::new("sh");
    command.args(["-c", script]).stdout(Stdio::piped());
    command
}

// A child that ignores the termination signal, so only a force kill stops it
async fn spawn_ignoring_term(set: &ProcessSet) -> SharedChild {
    spawn_ignoring(set, "TERM").await
}

async fn spawn_ignoring(set: &ProcessSet, signal: &str) -> SharedChild {
    let child = set
        .spawn(shell(&format!(
            "trap '' {signal}; echo ready; exec sleep 30"
        )))
        .unwrap();

    let mut ready = String::new();
    BufReader::new(child.take_stdout().await.unwrap())
        .read_line(&mut ready)
        .await
        .unwrap();
    assert_eq!(ready, "ready\n");

    child
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
        let owner = set.spawn(TokioCommand::new("true")).unwrap();
        let pid = owner.id();

        assert!(matches!(
            wait_for_exit(&owner).await,
            ChildExit::Completed(status) if status.success()
        ));

        // Still tracked, so a shutdown can stop output readers that
        // outlive the child
        assert!(set.get(pid).is_some());

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
    async fn a_zero_grace_period_finishes_once_the_children_exit() {
        let set = ProcessSet::root().nested_with_grace(Duration::ZERO);
        let child = spawn_sleep(&set);

        timeout(Duration::from_secs(1), set.shutdown(SignalType::Terminate))
            .await
            .expect("shutdown did not finish once its children exited");

        assert_eq!(wait_for_exit(&child).await, ChildExit::Terminated(15));
        timeout(Duration::from_secs(1), set.wait_idle())
            .await
            .expect("exited child remained tracked");
    }

    #[tokio::test]
    async fn shutting_down_a_parent_uses_its_grace_period_for_nested_sets() {
        let parent = ProcessSet::root().nested_with_grace(Duration::from_millis(50));
        let nested = parent.nested_with_grace(Duration::ZERO);
        let child = spawn_sleep(&nested);

        timeout(
            Duration::from_secs(1),
            parent.shutdown(SignalType::Terminate),
        )
        .await
        .expect("a zero grace nested set held up its parent's shutdown");

        assert_eq!(wait_for_exit(&child).await, ChildExit::Terminated(15));
    }

    #[tokio::test]
    async fn a_second_shutdown_force_kills_without_waiting_for_the_grace_period() {
        let set = ProcessSet::root().nested_with_grace(Duration::from_secs(5));
        let child = set
            .spawn(shell("trap '' TERM; echo ready; exec sleep 30"))
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

    #[tokio::test]
    async fn a_second_shutdown_returns_once_it_has_force_killed() {
        let set = ProcessSet::root().nested_with_grace(Duration::from_secs(10));
        let child = spawn_ignoring_term(&set).await;
        let (started, first_started) = std::sync::mpsc::channel();

        // Run the first shutdown on a runtime that then stops running tasks,
        // so only the second request itself can force kill in the meantime
        let first = std::thread::spawn({
            let set = set.clone();

            move || {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap()
                    .block_on(async {
                        let shutdown = set.shutdown(SignalType::Terminate);
                        tokio::pin!(shutdown);

                        let _ = timeout(Duration::from_millis(50), &mut shutdown).await;

                        started.send(()).unwrap();
                        std::thread::sleep(Duration::from_millis(1500));

                        shutdown.await;
                    });
            }
        });

        first_started.recv().unwrap();

        // It must not wait on the stalled runtime, which holds the wait on
        // the child, to see it exit
        timeout(Duration::from_secs(1), set.shutdown(SignalType::Terminate))
            .await
            .expect("second shutdown waited on the stalled runtime");

        // Only untracked once it was seen to exit
        assert!(set.get(child.id()).is_none(), "child was still running");

        first.join().unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_shutdown_after_a_cancelled_one_still_acts() {
        let set = ProcessSet::root().nested_with_grace(Duration::from_secs(10));
        let child = spawn_ignoring_term(&set).await;

        // Inside a worker task, where the cancelled shutdown's cleanup and
        // the next shutdown can be scheduled in either order
        tokio::spawn({
            let set = set.clone();

            async move {
                let _ = timeout(
                    Duration::from_millis(100),
                    set.shutdown(SignalType::Terminate),
                )
                .await;

                set.shutdown(SignalType::Kill).await;
            }
        })
        .await
        .unwrap();

        assert_eq!(wait_for_exit(&child).await, ChildExit::Killed);
    }

    #[tokio::test]
    async fn a_kill_shutdown_does_not_wait_for_the_grace_period() {
        let set = ProcessSet::root().nested_with_grace(Duration::from_secs(10));
        let child = spawn_sleep(&set);

        timeout(Duration::from_secs(1), set.shutdown(SignalType::Kill))
            .await
            .expect("kill shutdown waited for the grace period");

        assert_eq!(wait_for_exit(&child).await, ChildExit::Killed);
    }

    #[tokio::test]
    async fn force_killing_reaches_children_tracked_after_the_first_shutdown() {
        let parent = ProcessSet::root().nested_with_grace(Duration::from_secs(10));
        let busy = parent.nested();
        let empty = parent.nested();
        let first = spawn_ignoring_term(&busy).await;

        shutdown_in_background(&parent);
        sleep(Duration::from_millis(50)).await;

        // Tracked once the shutdown had begun: one in the set it was waiting
        // on, and one in a sibling set that was empty when it began
        let later = spawn_sleep(&busy);
        let sibling = spawn_sleep(&empty);

        timeout(
            Duration::from_secs(1),
            parent.shutdown(SignalType::Terminate),
        )
        .await
        .expect("second shutdown did not finish");

        assert_eq!(wait_for_exit(&first).await, ChildExit::Killed);
        assert_eq!(wait_for_exit(&later).await, ChildExit::Killed);
        assert_eq!(wait_for_exit(&sibling).await, ChildExit::Killed);
    }

    #[tokio::test]
    async fn shutting_down_a_parent_force_kills_a_nested_shutdown_in_progress() {
        let parent = ProcessSet::root().nested_with_grace(Duration::from_secs(10));
        let nested = parent.nested();
        let child = spawn_ignoring_term(&nested).await;

        shutdown_in_background(&nested);
        sleep(Duration::from_millis(50)).await;

        timeout(
            Duration::from_secs(1),
            parent.shutdown(SignalType::Terminate),
        )
        .await
        .expect("parent shutdown waited on a child it force killed");

        assert_eq!(wait_for_exit(&child).await, ChildExit::Killed);
    }

    #[tokio::test]
    async fn a_nested_shutdown_after_its_parents_does_not_signal_again() {
        let parent = ProcessSet::root().nested_with_grace(Duration::from_secs(10));
        let nested = parent.nested();

        // Reports each termination signal and keeps running
        let child = nested
            .spawn(shell(
                "trap 'echo term' TERM; echo ready; while :; do sleep 0.05; done",
            ))
            .unwrap();
        let mut stdout = BufReader::new(child.take_stdout().await.unwrap());
        let mut line = String::new();

        stdout.read_line(&mut line).await.unwrap();
        assert_eq!(line, "ready\n");

        // As an app does when it shuts down its own set in reaction to the
        // signal that shut down the root
        shutdown_in_background(&parent);
        sleep(Duration::from_millis(100)).await;
        shutdown_in_background(&nested);

        let mut signalled = 0;

        while let Ok(Ok(read)) =
            timeout(Duration::from_millis(400), stdout.read_line(&mut line)).await
        {
            if read == 0 {
                break;
            }

            signalled += 1;
        }

        assert_eq!(signalled, 1);
        assert!(
            timeout(Duration::from_millis(100), child.wait())
                .await
                .is_err(),
            "the nested shutdown force killed the child"
        );
    }

    #[tokio::test]
    async fn dropping_a_nested_set_keeps_its_children_reachable() {
        let parent = create_set();

        // The nested set's last handle is dropped straight away
        let shut_down = spawn_sleep(&parent.nested());
        let waited_on = spawn_sleep(&parent.nested());

        let idle = tokio::spawn({
            let parent = parent.clone();

            async move { parent.wait_idle().await }
        });

        sleep(Duration::from_millis(50)).await;
        drop(waited_on);
        shutdown_in_background(&parent);

        assert_eq!(wait_for_exit(&shut_down).await, ChildExit::Terminated(15));
        drop(shut_down);

        timeout(Duration::from_secs(1), idle)
            .await
            .expect("parent did not become idle")
            .unwrap();
    }

    #[tokio::test]
    async fn killing_all_reaches_nested_sets() {
        let parent = create_set();
        let nested = parent.nested();
        let child = spawn_sleep(&nested);

        parent.kill_all();

        assert_eq!(wait_for_exit(&child).await, ChildExit::Killed);
    }

    #[tokio::test]
    async fn dropping_the_owner_during_a_shutdown_lets_it_finish() {
        let set = ProcessSet::root().nested_with_grace(Duration::from_secs(10));
        let owner = spawn_ignoring_term(&set).await;
        let child = owner.clone();

        shutdown_in_background(&set);
        sleep(Duration::from_millis(50)).await;
        drop(owner);

        assert_eq!(wait_for_exit(&child).await, ChildExit::Killed);
        timeout(Duration::from_secs(1), set.wait_idle())
            .await
            .expect("cancelled child remained tracked");

        // Every child of the waiting shutdown has exited, so this starts a
        // new one, which has nothing to wait on
        timeout(Duration::from_secs(1), set.shutdown(SignalType::Terminate))
            .await
            .expect("new shutdown waited for the old grace period");
    }

    #[tokio::test]
    async fn a_waiter_sees_the_owner_kill_the_child() {
        let set = create_set();
        let owner = spawn_sleep(&set);
        let child = owner.clone();
        let waiting = tokio::spawn(async move { child.wait().await });

        tokio::task::yield_now().await;
        drop(owner);

        assert_eq!(
            timeout(Duration::from_secs(1), waiting)
                .await
                .expect("waiter was not woken")
                .unwrap()
                .unwrap(),
            ChildExit::Killed
        );
    }

    #[tokio::test]
    async fn stopping_cleanup_leaves_the_child_running_but_untracked() {
        let set = create_set();
        let owner = spawn_sleep(&set);
        let child = owner.clone();
        let pid = owner.id();

        owner.stop_cleanup();
        drop(owner);

        assert!(set.get(pid).is_none());
        assert!(
            timeout(Duration::from_millis(100), child.wait())
                .await
                .is_err()
        );

        child.kill().await.unwrap();
    }

    #[tokio::test]
    async fn a_signal_sent_outside_a_shutdown_does_not_stop_it_signalling() {
        let set = ProcessSet::root().nested_with_grace(Duration::from_secs(10));
        let child = spawn_ignoring(&set, "HUP").await;

        // As an app does to have a server reload its config
        child.send_signal(SignalType::Hangup).unwrap();

        shutdown_in_background(&set);

        assert_eq!(wait_for_exit(&child).await, ChildExit::Terminated(15));
    }

    #[tokio::test]
    async fn a_cancelled_shutdown_does_not_make_the_next_one_force_kill() {
        let set = ProcessSet::root().nested_with_grace(Duration::from_secs(10));
        let child = spawn_ignoring_term(&set).await;

        let _ = timeout(
            Duration::from_millis(50),
            set.shutdown(SignalType::Terminate),
        )
        .await;

        // A new graceful shutdown, not an escalation of the cancelled one
        shutdown_in_background(&set);

        assert!(
            timeout(Duration::from_millis(200), child.wait())
                .await
                .is_err(),
            "the child was force killed"
        );

        child.kill().await.unwrap();
    }

    #[test]
    fn shutdowns_requested_at_the_same_time_still_escalate() {
        let set = ProcessSet::root().nested_with_grace(Duration::from_secs(10));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        // Several children widen the window between one request checking for
        // a shutdown in progress and starting its own
        let children = runtime.block_on(async {
            let mut children = vec![];

            for _ in 0..10 {
                children.push(spawn_ignoring_term(&set).await);
            }

            children
        });

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let requests = (0..2)
            .map(|_| {
                let set = set.clone();
                let barrier = std::sync::Arc::clone(&barrier);

                std::thread::spawn(move || {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap();

                    barrier.wait();
                    runtime.block_on(set.shutdown(SignalType::Terminate));
                })
            })
            .collect::<Vec<_>>();

        runtime.block_on(async {
            for child in &children {
                assert_eq!(wait_for_exit(child).await, ChildExit::Killed);
            }
        });

        for request in requests {
            request.join().unwrap();
        }
    }

    #[tokio::test]
    async fn releasing_a_child_during_a_shutdown_does_not_hang_it() {
        let set = ProcessSet::root().nested_with_grace(Duration::from_secs(1));
        let owner = spawn_ignoring_term(&set).await;
        let child = owner.clone();

        let first = tokio::spawn({
            let set = set.clone();

            async move { set.shutdown(SignalType::Terminate).await }
        });

        sleep(Duration::from_millis(50)).await;

        // Released, so it keeps running but no longer counts as running here
        owner.stop_cleanup();
        drop(owner);

        set.shutdown(SignalType::Terminate).await;

        timeout(Duration::from_secs(3), first)
            .await
            .expect("the first shutdown never finished")
            .unwrap();

        child.kill().await.unwrap();
    }
}
