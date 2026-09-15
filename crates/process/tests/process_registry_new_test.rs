#![cfg(unix)]

use starbase_process::process_registry_new::{
    ChildState, ProcessEvent, ProcessRegistry, ProcessRegistryOptions,
};
use starbase_process::{ChildExit, Output, SharedChild, SignalType};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::timeout;

fn spawn_sleep() -> Child {
    Command::new("sleep").arg("30").spawn().unwrap()
}

// Prints "ready" once the child is running, then ignores `SIGTERM`.
fn spawn_stubborn() -> Child {
    Command::new("sh")
        .args(["-c", "trap '' TERM; echo ready; exec sleep 30"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap()
}

// A shell wrapper whose work runs in a grandchild. Prints the grandchild
// pid once it is running.
fn spawn_wrapper() -> Child {
    Command::new("sh")
        .args(["-c", "sleep 30 & echo $!; wait"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap()
}

async fn read_line(child: &SharedChild) -> String {
    let mut line = String::new();

    timeout(
        Duration::from_secs(3),
        BufReader::new(child.take_stdout().await.unwrap()).read_line(&mut line),
    )
    .await
    .unwrap()
    .unwrap();

    line.trim().to_owned()
}

fn is_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

async fn wait_until(mut check: impl AsyncFnMut() -> bool) {
    timeout(Duration::from_secs(3), async {
        while !check().await {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("condition was not met in time");
}

fn options() -> ProcessRegistryOptions {
    ProcessRegistryOptions {
        shutdown_threshold: Duration::from_millis(200),
        // Tests own their registries, so don't hook the test runner's signals
        handle_signals: false,
        reap_interval: Duration::from_millis(50),
        ..Default::default()
    }
}

fn create_registry() -> ProcessRegistry {
    ProcessRegistry::with_options(options())
}

mod lifecycle {
    use super::*;

    #[tokio::test]
    async fn instance_is_a_singleton() {
        assert!(Arc::ptr_eq(
            &ProcessRegistry::instance(),
            &ProcessRegistry::instance()
        ));
    }

    #[tokio::test]
    async fn starts_inside_a_runtime() {
        let registry = create_registry();

        assert!(registry.is_started());
        assert!(!registry.start());
        assert!(registry.stop());
        assert!(!registry.stop());
        assert!(registry.start());
    }

    #[test]
    fn does_not_start_outside_a_runtime() {
        let registry = create_registry();

        assert!(!registry.is_started());

        let runtime = tokio::runtime::Runtime::new().unwrap();

        runtime.block_on(async {
            assert!(registry.start());
            assert!(registry.is_started());
        });

        registry.stop();
    }

    #[tokio::test]
    async fn restart_keeps_state() {
        let registry = create_registry();
        let child = registry.track(spawn_sleep()).await;
        let pid = child.id();

        registry.cache_output("key", fake_output()).await;

        assert!(registry.restart());
        assert!(registry.is_started());
        assert!(registry.get_running_by_pid(pid).await.is_some());
        assert!(registry.get_cached_output("key").await.is_some());

        // Exits are still detected after the restart
        child.kill().await.unwrap();

        wait_until(async || registry.get_running_by_pid(pid).await.is_none()).await;
    }

    #[tokio::test]
    async fn emits_start_and_stop_events() {
        let registry = create_registry();
        let mut events = registry.subscribe_events();

        registry.stop();
        registry.start();

        assert!(matches!(
            events.recv().await.unwrap(),
            ProcessEvent::Stopped
        ));
        assert!(matches!(
            events.recv().await.unwrap(),
            ProcessEvent::Started
        ));
    }
}

mod tracking {
    use super::*;

    #[tokio::test]
    async fn tracks_and_releases_children() {
        let registry = create_registry();
        let child = registry.track(spawn_sleep()).await;
        let pid = child.id();

        assert!(registry.get(pid).await.is_some_and(|c| c.is_running()));
        assert!(registry.get_running_by_pid(pid).await.is_some());
        assert_eq!(registry.running_count(), 1);
        assert_eq!(registry.list().await.len(), 1);

        assert!(registry.release(&child).await.is_some());
        assert!(registry.release(&child).await.is_none());

        assert!(registry.get(pid).await.is_none());
        assert_eq!(registry.running_count(), 0);

        // Releasing doesn't kill
        assert!(is_alive(pid));

        child.kill().await.unwrap();
    }

    #[tokio::test]
    async fn spawns_and_records_the_command() {
        let registry = create_registry();
        let child = registry
            .spawn(Command::new("sleep").arg("30"))
            .await
            .unwrap();

        let tracked = registry.get(child.id()).await.unwrap();

        assert_eq!(tracked.command.as_deref(), Some("sleep 30"));

        child.kill().await.unwrap();
    }

    #[tokio::test]
    async fn spawn_errors_are_returned() {
        let registry = create_registry();

        assert!(
            registry
                .spawn(&mut Command::new("starbase-does-not-exist"))
                .await
                .is_err()
        );
        assert!(registry.list().await.is_empty());
    }

    #[tokio::test]
    async fn unknown_pids_are_not_tracked() {
        let registry = create_registry();

        assert!(registry.get(0).await.is_none());
        assert!(registry.get_running_by_pid(0).await.is_none());
        assert!(registry.release_by_pid(0).await.is_none());
        assert!(registry.kill(0).await.unwrap().is_none());
        assert!(!registry.signal(0, SignalType::Terminate).await.unwrap());
    }

    #[tokio::test]
    async fn reaps_exited_children_in_the_background() {
        let registry = create_registry();
        let mut events = registry.subscribe_events();
        let child = registry.track(Command::new("true").spawn().unwrap()).await;
        let pid = child.id();

        wait_until(async || registry.get_running_by_pid(pid).await.is_none()).await;

        // Stays tracked, as exited, until released
        let tracked = registry.get(pid).await.unwrap();

        assert!(matches!(
            tracked.state,
            ChildState::Exited(ChildExit::Completed(status)) if status.success()
        ));
        assert_eq!(registry.running_count(), 0);

        // The owner can still wait on it
        assert!(matches!(
            child.wait().await.unwrap(),
            ChildExit::Completed(_)
        ));

        let mut seen_exit = false;

        while let Ok(event) = events.try_recv() {
            if let ProcessEvent::Exited { pid: event_pid, .. } = event {
                assert_eq!(event_pid, pid);
                seen_exit = true;
            }
        }

        assert!(seen_exit);
    }

    #[tokio::test]
    async fn reaping_does_not_close_stdin() {
        use tokio::io::AsyncWriteExt;

        let registry = create_registry();
        let child = registry
            .track(
                Command::new("cat")
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .spawn()
                    .unwrap(),
            )
            .await;

        // Give the reaper a chance to run before taking stdin
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut stdin = child
            .take_stdin()
            .await
            .expect("stdin was closed by the reaper");
        stdin.write_all(b"hello").await.unwrap();
        drop(stdin);

        let output = timeout(Duration::from_secs(3), child.wait_with_output())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(output.stdout.as_ref(), b"hello");
        assert!(output.success());
    }

    #[tokio::test]
    async fn wait_for_idle_returns_once_children_exit() {
        let registry = create_registry();

        // Nothing running
        timeout(Duration::from_secs(1), registry.wait_for_idle())
            .await
            .unwrap();

        let child = registry
            .track(Command::new("sleep").arg("0.2").spawn().unwrap())
            .await;

        timeout(Duration::from_secs(3), registry.wait_for_idle())
            .await
            .unwrap();

        assert!(registry.get_running_by_pid(child.id()).await.is_none());
    }

    #[tokio::test]
    async fn dropping_a_running_handle_kills_the_child() {
        let registry = create_registry();
        let child = registry.track(spawn_sleep()).await;
        let pid = child.id();
        let clone = child.clone();

        // Clones never trigger cleanup
        drop(clone);

        tokio::time::sleep(Duration::from_millis(50)).await;

        assert!(registry.get_running_by_pid(pid).await.is_some());

        drop(child);

        wait_until(async || registry.get(pid).await.is_none()).await;

        assert!(!is_alive(pid));
    }

    #[tokio::test]
    async fn dropping_a_running_handle_can_leave_the_child_alone() {
        let registry = ProcessRegistry::with_options(ProcessRegistryOptions {
            kill_on_drop: false,
            ..options()
        });
        let child = registry.track(spawn_sleep()).await;
        let pid = child.id();

        drop(child);

        wait_until(async || registry.get(pid).await.is_none()).await;

        assert!(is_alive(pid));

        starbase_process::kill(pid, SignalType::Kill).unwrap();
    }

    #[tokio::test]
    async fn dropping_an_exited_handle_releases_it() {
        let registry = create_registry();
        let child = registry.track(Command::new("true").spawn().unwrap()).await;
        let pid = child.id();

        child.wait().await.unwrap();

        assert!(registry.get(pid).await.is_some());

        drop(child);

        wait_until(async || registry.get(pid).await.is_none()).await;
    }

    #[tokio::test]
    async fn dropping_the_registry_kills_tracked_children() {
        let registry = create_registry();
        let child = registry.track(spawn_sleep()).await;

        drop(registry);

        assert_eq!(
            timeout(Duration::from_secs(1), child.wait())
                .await
                .unwrap()
                .unwrap(),
            ChildExit::Killed
        );
    }
}

mod signals {
    use super::*;

    #[tokio::test]
    async fn signals_a_single_child() {
        let registry = create_registry();
        let child = registry.track(spawn_sleep()).await;

        assert!(
            registry
                .signal(child.id(), SignalType::Terminate)
                .await
                .unwrap()
        );
        assert_eq!(child.wait().await.unwrap(), ChildExit::Terminated(15));
    }

    #[tokio::test]
    async fn kills_a_single_child_and_waits() {
        let registry = create_registry();
        let child = registry.track(spawn_sleep()).await;

        assert_eq!(
            registry.kill(child.id()).await.unwrap(),
            Some(ChildExit::Killed)
        );
    }

    #[tokio::test]
    async fn signals_descendants() {
        let registry = create_registry();
        let child = registry.track(spawn_wrapper()).await;
        let grandchild: u32 = read_line(&child).await.parse().unwrap();

        assert!(is_alive(grandchild));

        registry
            .signal(child.id(), SignalType::Terminate)
            .await
            .unwrap();

        assert_eq!(child.wait().await.unwrap(), ChildExit::Terminated(15));

        wait_until(async || !is_alive(grandchild)).await;
    }

    #[tokio::test]
    async fn can_leave_descendants_alone() {
        let registry = ProcessRegistry::with_options(ProcessRegistryOptions {
            signal_descendants: false,
            ..options()
        });
        let child = registry.track(spawn_wrapper()).await;
        let grandchild: u32 = read_line(&child).await.parse().unwrap();

        registry
            .signal(child.id(), SignalType::Terminate)
            .await
            .unwrap();

        assert_eq!(child.wait().await.unwrap(), ChildExit::Terminated(15));
        assert!(is_alive(grandchild));

        starbase_process::kill(grandchild, SignalType::Kill).unwrap();
    }

    #[tokio::test]
    async fn signals_all_running_children() {
        let registry = create_registry();
        let first = registry.track(spawn_sleep()).await;
        let second = registry.track(spawn_sleep()).await;

        registry.signal_running(SignalType::Interrupt).await;

        assert_eq!(first.wait().await.unwrap(), ChildExit::Interrupted);
        assert_eq!(second.wait().await.unwrap(), ChildExit::Interrupted);
    }

    #[tokio::test]
    async fn broadcasts_signals_to_subscribers() {
        let registry = create_registry();
        let mut first = registry.subscribe_signals();
        let mut second = registry.subscribe_signals();

        registry.terminate_running();

        assert!(matches!(first.recv().await.unwrap(), SignalType::Terminate));
        assert!(matches!(
            second.recv().await.unwrap(),
            SignalType::Terminate
        ));
    }
}

mod shutdown {
    use super::*;

    #[tokio::test]
    async fn shuts_down_gracefully() {
        let registry = create_registry();
        let first = registry.track(spawn_sleep()).await;
        let second = registry.track(spawn_sleep()).await;

        timeout(
            Duration::from_secs(3),
            registry.shutdown(SignalType::Terminate),
        )
        .await
        .unwrap();

        assert_eq!(first.wait().await.unwrap(), ChildExit::Terminated(15));
        assert_eq!(second.wait().await.unwrap(), ChildExit::Terminated(15));
        assert_eq!(registry.running_count(), 0);

        // Children stay tracked until released
        assert_eq!(registry.list().await.len(), 2);
    }

    #[tokio::test]
    async fn shutdown_with_nothing_running_returns_immediately() {
        let registry = create_registry();

        timeout(
            Duration::from_secs(1),
            registry.shutdown(SignalType::Terminate),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn force_kills_after_the_threshold() {
        let registry = create_registry();
        let child = registry.track(spawn_stubborn()).await;
        let mut events = registry.subscribe_events();

        assert_eq!(read_line(&child).await, "ready");

        timeout(
            Duration::from_secs(3),
            registry.shutdown(SignalType::Terminate),
        )
        .await
        .unwrap();

        assert_eq!(child.wait().await.unwrap(), ChildExit::Killed);

        let mut forced = false;

        while let Ok(event) = events.try_recv() {
            if matches!(event, ProcessEvent::ShutdownForced { .. }) {
                forced = true;
            }
        }

        assert!(forced);
    }

    #[tokio::test]
    async fn a_second_signal_force_kills_without_waiting() {
        let registry = ProcessRegistry::with_options(ProcessRegistryOptions {
            shutdown_threshold: Duration::from_secs(30),
            ..options()
        });
        let child = registry.track(spawn_stubborn()).await;

        assert_eq!(read_line(&child).await, "ready");

        registry.terminate_running();
        registry.terminate_running();

        assert_eq!(
            timeout(Duration::from_secs(2), child.wait())
                .await
                .expect("second signal did not bypass the threshold")
                .unwrap(),
            ChildExit::Killed
        );
    }

    #[tokio::test]
    async fn zero_threshold_waits_for_children() {
        let registry = ProcessRegistry::with_options(ProcessRegistryOptions {
            shutdown_threshold: Duration::ZERO,
            ..options()
        });
        let child = registry.track(spawn_sleep()).await;

        timeout(
            Duration::from_secs(3),
            registry.shutdown(SignalType::Terminate),
        )
        .await
        .unwrap();

        assert_eq!(child.wait().await.unwrap(), ChildExit::Terminated(15));
    }

    #[tokio::test]
    async fn shutdown_handles_later_children() {
        let registry = create_registry();

        let first = registry.track(spawn_sleep()).await;
        registry.shutdown(SignalType::Terminate).await;
        assert_eq!(first.wait().await.unwrap(), ChildExit::Terminated(15));

        let second = registry.track(spawn_sleep()).await;
        registry.shutdown(SignalType::Terminate).await;
        assert_eq!(second.wait().await.unwrap(), ChildExit::Terminated(15));
    }

    #[tokio::test]
    async fn shutdown_signals_descendants() {
        let registry = create_registry();
        let child = registry.track(spawn_wrapper()).await;
        let grandchild: u32 = read_line(&child).await.parse().unwrap();

        timeout(
            Duration::from_secs(3),
            registry.shutdown(SignalType::Terminate),
        )
        .await
        .unwrap();

        wait_until(async || !is_alive(grandchild)).await;
    }

    #[tokio::test]
    async fn shutdown_runs_inline_while_stopped() {
        let registry = create_registry();
        let child = registry.track(spawn_stubborn()).await;

        assert_eq!(read_line(&child).await, "ready");

        registry.stop();

        timeout(
            Duration::from_secs(3),
            registry.shutdown(SignalType::Terminate),
        )
        .await
        .unwrap();

        assert_eq!(child.wait().await.unwrap(), ChildExit::Killed);
        assert_eq!(registry.running_count(), 0);
    }

    #[tokio::test]
    async fn emits_shutdown_events() {
        let registry = create_registry();
        let mut events = registry.subscribe_events();
        let child = registry.track(spawn_sleep()).await;
        let pid = child.id();

        registry.shutdown(SignalType::Terminate).await;

        let mut seen = vec![];

        while let Ok(event) = events.try_recv() {
            seen.push(event);
        }

        assert!(matches!(seen[0], ProcessEvent::Tracked { pid: p } if p == pid));
        assert!(matches!(
            seen[1],
            ProcessEvent::Signal(SignalType::Terminate)
        ));
        assert!(matches!(
            &seen[2],
            ProcessEvent::ShutdownStarted { signal: SignalType::Terminate, pids } if pids == &[pid]
        ));
        assert!(
            matches!(seen[3], ProcessEvent::Exited { pid: p, exit: ChildExit::Terminated(15) } if p == pid)
        );
        assert!(matches!(seen[4], ProcessEvent::ShutdownFinished));
    }

    #[tokio::test]
    async fn force_kill_stops_capture_of_pipes_held_by_descendants() {
        use std::future::{Future, poll_fn};
        use std::task::Poll;

        let registry = create_registry();

        // The shell exits, but its background child keeps stdout open
        let child = registry
            .track(
                Command::new("sh")
                    .args(["-c", "sleep 30 & echo $! >&2; printf ready; exit 7"])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap(),
            )
            .await;

        let mut pid = String::new();
        BufReader::new(child.take_stderr().await.unwrap())
            .read_line(&mut pid)
            .await
            .unwrap();
        let grandchild: u32 = pid.trim().parse().unwrap();

        let exit = child.wait().await.unwrap();
        assert!(matches!(exit, ChildExit::Completed(status) if status.code() == Some(7)));

        let capture = child.wait_with_output();
        tokio::pin!(capture);

        // Consume what was written, then block on the pipe held by the grandchild
        poll_fn(|cx| {
            assert!(capture.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;

        // The child has exited, but is still tracked, so the force kill
        // stops its readers
        registry.shutdown(SignalType::Kill).await;

        let output = timeout(Duration::from_secs(1), capture)
            .await
            .expect("capture waited on a descendant after forced shutdown")
            .unwrap();

        assert_eq!(output.exit, exit);
        assert_eq!(output.stdout.as_ref(), b"ready");

        starbase_process::kill(grandchild, SignalType::Kill).unwrap();
    }
}

mod cache {
    use super::*;

    #[tokio::test]
    async fn caches_and_removes_output() {
        let registry = create_registry();

        assert!(registry.get_cached_output("key").await.is_none());

        registry.cache_output("key", fake_output()).await;

        assert_eq!(registry.get_cached_output("key").await, Some(fake_output()));

        assert_eq!(
            registry.remove_cached_output("key").await,
            Some(fake_output())
        );
        assert!(registry.get_cached_output("key").await.is_none());

        registry.cache_output("key", fake_output()).await;
        registry.clear_cache().await;

        assert!(registry.get_cached_output("key").await.is_none());
    }

    #[tokio::test]
    async fn exec_cached_runs_once_per_key() {
        let registry = Arc::new(create_registry());
        let runs = Arc::new(AtomicUsize::new(0));
        let mut handles = vec![];

        for _ in 0..5 {
            let registry = Arc::clone(&registry);
            let runs = Arc::clone(&runs);

            handles.push(tokio::spawn(async move {
                registry
                    .exec_cached("key", async {
                        runs.fetch_add(1, Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        Ok::<_, ()>(fake_output())
                    })
                    .await
            }));
        }

        for handle in handles {
            assert_eq!(handle.await.unwrap(), Ok(fake_output()));
        }

        assert_eq!(runs.load(Ordering::SeqCst), 1);
        assert_eq!(registry.get_cached_output("key").await, Some(fake_output()));

        // Served from the cache now
        let output = registry
            .exec_cached("key", async {
                runs.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ()>(fake_output())
            })
            .await;

        assert_eq!(output, Ok(fake_output()));
        assert_eq!(runs.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn exec_cached_does_not_cache_failures() {
        let registry = Arc::new(create_registry());

        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (fail_tx, fail_rx) = tokio::sync::oneshot::channel::<()>();

        let runner = {
            let registry = Arc::clone(&registry);

            tokio::spawn(async move {
                registry
                    .exec_cached("key", async {
                        started_tx.send(()).unwrap();
                        fail_rx.await.unwrap();
                        Err::<Output, &str>("boom")
                    })
                    .await
            })
        };

        started_rx.await.unwrap();

        let waiter = {
            let registry = Arc::clone(&registry);

            tokio::spawn(async move {
                registry
                    .exec_cached("key", async { Ok::<_, &str>(fake_output()) })
                    .await
            })
        };

        // Give the waiter time to attach to the in-flight run
        tokio::time::sleep(Duration::from_millis(50)).await;
        fail_tx.send(()).unwrap();

        assert_eq!(runner.await.unwrap(), Err("boom"));

        // The waiter ran on its own after the failure
        assert_eq!(waiter.await.unwrap(), Ok(fake_output()));
        assert_eq!(registry.get_cached_output("key").await, Some(fake_output()));
    }
}

fn fake_output() -> Output {
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    Output {
        exit: ChildExit::Completed(ExitStatus::from_raw(0)),
        stdout: "out".into(),
        stderr: "err".into(),
    }
}
