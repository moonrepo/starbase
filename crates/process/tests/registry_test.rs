use starbase_process::registry::{
    ChildState, ProcessEvent, ProcessRegistry, ProcessRegistryOptions,
};
use starbase_process::{ChildExit, Output, SharedChild, SignalType};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::io::AsyncReadExt;
#[cfg(unix)]
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStdout, Command};
use tokio::time::timeout;

// Runs for 30 seconds unless it is stopped.
fn spawn_sleep() -> Child {
    #[cfg(unix)]
    let mut command = {
        let mut command = Command::new("sleep");
        command.arg("30");
        command
    };
    #[cfg(windows)]
    let mut command = {
        let mut command = Command::new("ping");
        command
            .args(["-n", "30", "127.0.0.1"])
            .stdout(Stdio::null());
        command
    };

    command.spawn().unwrap()
}

// Exits successfully right away.
fn spawn_exit() -> Child {
    #[cfg(unix)]
    let mut command = Command::new("true");
    #[cfg(windows)]
    let mut command = {
        let mut command = Command::new("cmd.exe");
        command.args(["/D", "/C", "exit 0"]);
        command
    };

    command.spawn().unwrap()
}

// A wrapper whose work runs in a grandchild that inherits its stdout pipe.
// Something is written to the pipe once the grandchild is running, and it
// only reaches end of file once neither process holds it open any more.
fn spawn_piped_wrapper() -> Child {
    #[cfg(unix)]
    let mut command = {
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 30 & echo ready; wait"]);
        command
    };
    #[cfg(windows)]
    let mut command = {
        let mut command = Command::new("cmd.exe");
        command.args(["/D", "/C", "ping -n 30 127.0.0.1"]);
        command
    };

    command.stdout(Stdio::piped()).spawn().unwrap()
}

// Wait for the grandchild of a piped wrapper to be running, and return the
// pipe that it holds open.
async fn wait_for_grandchild(child: &SharedChild) -> ChildStdout {
    let mut stdout = child.take_stdout().await.unwrap();
    let mut buffer = [0; 64];

    let read = timeout(Duration::from_secs(10), stdout.read(&mut buffer))
        .await
        .expect("grandchild did not start")
        .unwrap();

    assert!(read > 0);

    stdout
}

// Passes once every process holding the pipe has gone, which is how these
// tests observe that a descendant was terminated without knowing its pid.
async fn assert_pipe_closes(mut stdout: ChildStdout) {
    let mut rest = vec![];

    // Only the end of file matters, not how it is reported
    let _ = timeout(Duration::from_secs(5), stdout.read_to_end(&mut rest))
        .await
        .expect("a descendant survived, and still holds the pipe open");
}

// Prints "ready" once the child is running, then ignores `SIGTERM`.
#[cfg(unix)]
fn spawn_stubborn() -> Child {
    Command::new("sh")
        .args(["-c", "trap '' TERM; echo ready; exec sleep 30"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap()
}

// A shell wrapper whose work runs in a grandchild. Prints the grandchild
// pid once it is running.
#[cfg(unix)]
fn spawn_wrapper() -> Child {
    Command::new("sh")
        .args(["-c", "sleep 30 & echo $!; wait"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap()
}

#[cfg(unix)]
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

#[cfg(unix)]
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
    async fn register_does_not_replace_the_singleton() {
        let instance = ProcessRegistry::instance();

        assert!(!ProcessRegistry::register(options()));
        assert!(Arc::ptr_eq(&instance, &ProcessRegistry::instance()));
        assert!(Arc::ptr_eq(
            &instance,
            &ProcessRegistry::try_instance().unwrap()
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
        assert!(registry.get_running(pid).await.is_some());
        assert!(registry.get_cached_output("key").await.is_some());

        // Exits are still detected after the restart
        child.kill().await.unwrap();

        wait_until(async || registry.get_running(pid).await.is_none()).await;
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

// These rely on pids, signals a child can ignore, and Unix commands.
#[cfg(unix)]
mod tracking {
    use super::*;
    use starbase_process::registry::ChildSelector;

    #[tokio::test]
    async fn tracks_and_releases_children() {
        let registry = create_registry();
        let child = registry.track(spawn_sleep()).await;
        let pid = child.id();

        assert!(registry.get(pid).await.is_some_and(|c| c.is_running()));
        assert!(registry.get_running(pid).await.is_some());
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
    async fn selects_children_by_pid_or_handle() {
        let registry = create_registry();
        let child = registry.track(spawn_sleep()).await;
        let pid = child.id();

        assert!(registry.get(pid).await.is_some());
        assert!(registry.get(&child).await.is_some());
        assert!(registry.get(&child.clone()).await.is_some());
        assert!(registry.get_running(pid).await.is_some());
        assert!(registry.get_running(&child).await.is_some());

        assert!(registry.release(pid).await.is_some());
        assert!(registry.get(&child).await.is_none());

        child.kill().await.unwrap();
    }

    #[tokio::test]
    async fn handles_only_select_their_own_child() {
        let registry = create_registry();
        let child = registry.track(spawn_sleep()).await;
        let other = SharedChild::new(spawn_sleep());
        let tracked = registry.get(&child).await.unwrap();

        // By pid, by handle, and by a clone of the handle
        assert!(ChildSelector::from(child.id()).matches(&tracked));
        assert!(ChildSelector::from(&child).matches(&tracked));
        assert!(ChildSelector::from(&child.clone()).matches(&tracked));

        // A handle to another child never selects it, which is what
        // protects a child that reuses a pid from a stale handle
        assert!(!ChildSelector::from(&other).matches(&tracked));
        assert!(registry.get(&other).await.is_none());
        assert!(registry.release(&other).await.is_none());
        assert!(registry.kill(&other).await.unwrap().is_none());
        assert!(!registry.signal(&other, SignalType::Kill).await.unwrap());

        assert!(registry.get_running(&child).await.is_some());

        child.kill().await.unwrap();
        other.kill().await.unwrap();
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
        assert!(registry.get_running(0).await.is_none());
        assert!(registry.release(0).await.is_none());
        assert!(registry.kill(0).await.unwrap().is_none());
        assert!(!registry.signal(0, SignalType::Terminate).await.unwrap());
    }

    #[tokio::test]
    async fn reaps_exited_children_in_the_background() {
        let registry = create_registry();
        let mut events = registry.subscribe_events();
        let child = registry.track(Command::new("true").spawn().unwrap()).await;
        let pid = child.id();

        wait_until(async || registry.get_running(pid).await.is_none()).await;

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

        assert!(registry.get_running(child.id()).await.is_none());
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

        assert!(registry.get_running(pid).await.is_some());

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
    async fn dropping_the_registry_kills_descendants() {
        let registry = create_registry();
        let first = registry.track(spawn_wrapper()).await;
        let second = registry.track(spawn_wrapper()).await;
        let first_grandchild: u32 = read_line(&first).await.parse().unwrap();
        let second_grandchild: u32 = read_line(&second).await.parse().unwrap();

        drop(registry);

        assert_eq!(first.wait().await.unwrap(), ChildExit::Killed);
        assert_eq!(second.wait().await.unwrap(), ChildExit::Killed);

        wait_until(async || !is_alive(first_grandchild) && !is_alive(second_grandchild)).await;
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

#[cfg(unix)]
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
    async fn signals_and_kills_by_handle() {
        let registry = create_registry();
        let first = registry.track(spawn_sleep()).await;
        let second = registry.track(spawn_sleep()).await;

        assert!(
            registry
                .signal(&first, SignalType::Terminate)
                .await
                .unwrap()
        );
        assert_eq!(first.wait().await.unwrap(), ChildExit::Terminated(15));

        assert_eq!(
            registry.kill(&second).await.unwrap(),
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

#[cfg(unix)]
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
    async fn shutdown_signals_descendants_of_every_child() {
        let registry = create_registry();
        let first = registry.track(spawn_wrapper()).await;
        let second = registry.track(spawn_wrapper()).await;
        let first_grandchild: u32 = read_line(&first).await.parse().unwrap();
        let second_grandchild: u32 = read_line(&second).await.parse().unwrap();

        timeout(
            Duration::from_secs(3),
            registry.shutdown(SignalType::Terminate),
        )
        .await
        .unwrap();

        wait_until(async || !is_alive(first_grandchild) && !is_alive(second_grandchild)).await;
    }

    #[tokio::test]
    async fn shutdown_during_a_shutdown_escalates_and_waits() {
        let registry = ProcessRegistry::with_options(ProcessRegistryOptions {
            shutdown_threshold: Duration::from_secs(30),
            ..options()
        });
        let child = registry.track(spawn_stubborn()).await;

        assert_eq!(read_line(&child).await, "ready");

        // Starts a graceful shutdown that the child ignores
        registry.terminate_running();

        timeout(
            Duration::from_secs(3),
            registry.shutdown(SignalType::Terminate),
        )
        .await
        .expect("shutdown did not escalate the one in progress");

        assert_eq!(registry.running_count(), 0);
        assert_eq!(child.wait().await.unwrap(), ChildExit::Killed);
    }

    #[tokio::test]
    async fn shutdown_waits_for_children_tracked_during_a_shutdown() {
        let registry = create_registry();
        let first = registry.track(spawn_sleep()).await;

        registry.terminate_running();

        // Not part of the shutdown above, which may finish first
        let second = registry.track(spawn_sleep()).await;

        timeout(
            Duration::from_secs(3),
            registry.shutdown(SignalType::Terminate),
        )
        .await
        .unwrap();

        assert_eq!(registry.running_count(), 0);
        assert!(registry.get_running(&first).await.is_none());
        assert!(registry.get_running(&second).await.is_none());
    }

    #[tokio::test]
    async fn concurrent_shutdowns_wait_for_children() {
        let registry = create_registry();
        let _first = registry.track(spawn_sleep()).await;
        let _second = registry.track(spawn_sleep()).await;

        timeout(Duration::from_secs(3), async {
            tokio::join!(
                registry.shutdown(SignalType::Terminate),
                registry.shutdown(SignalType::Terminate)
            )
        })
        .await
        .unwrap();

        assert_eq!(registry.running_count(), 0);
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

        // The shutdown returns once the children are gone, which can be
        // just before the coordinator reports that it has finished
        let mut seen = vec![];

        timeout(Duration::from_secs(3), async {
            loop {
                let event = events.recv().await.unwrap();
                let finished = matches!(event, ProcessEvent::ShutdownFinished);

                seen.push(event);

                if finished {
                    break;
                }
            }
        })
        .await
        .unwrap();

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

// These run everywhere, Windows included, so they keep to what both
// platforms can express: no grandchild pids, and no signals that a child
// can ignore. A descendant is observed through a pipe that it inherits.
mod cross_platform {
    use super::*;

    #[tokio::test]
    async fn tracks_and_releases_children() {
        let registry = create_registry();
        let child = registry.track(spawn_sleep()).await;
        let pid = child.id();

        assert!(registry.get(pid).await.is_some_and(|c| c.is_running()));
        assert!(registry.get_running(&child).await.is_some());
        assert_eq!(registry.running_count(), 1);

        assert!(registry.release(&child).await.is_some());

        assert!(registry.get(pid).await.is_none());
        assert_eq!(registry.running_count(), 0);

        // Releasing doesn't kill, so this is the first signal it receives
        assert_eq!(child.kill().await.unwrap(), ChildExit::Killed);
    }

    #[tokio::test]
    async fn reaps_exited_children_in_the_background() {
        let registry = create_registry();
        let child = registry.track(spawn_exit()).await;

        wait_until(async || registry.get_running(&child).await.is_none()).await;

        // Stays tracked, as exited, until released
        let tracked = registry.get(&child).await.unwrap();

        assert!(matches!(
            tracked.state,
            ChildState::Exited(ChildExit::Completed(status)) if status.success()
        ));
        assert_eq!(registry.running_count(), 0);
    }

    #[tokio::test]
    async fn kills_a_child_and_waits() {
        let registry = create_registry();
        let child = registry.track(spawn_sleep()).await;

        assert_eq!(
            registry.kill(&child).await.unwrap(),
            Some(ChildExit::Killed)
        );

        wait_until(async || registry.get_running(&child).await.is_none()).await;
    }

    #[tokio::test]
    async fn dropping_a_running_handle_kills_the_child() {
        let registry = create_registry();
        let child = registry.track(spawn_sleep()).await;

        // Clones never trigger cleanup, but can observe it
        let clone = child.clone();

        drop(child);

        assert_eq!(
            timeout(Duration::from_secs(3), clone.wait())
                .await
                .unwrap()
                .unwrap(),
            ChildExit::Killed
        );

        wait_until(async || registry.get(&clone).await.is_none()).await;
    }

    #[tokio::test]
    async fn shuts_down_running_children() {
        let registry = create_registry();
        let first = registry.track(spawn_sleep()).await;
        let second = registry.track(spawn_sleep()).await;

        timeout(
            Duration::from_secs(5),
            registry.shutdown(SignalType::Terminate),
        )
        .await
        .unwrap();

        assert_eq!(registry.running_count(), 0);
        assert_eq!(first.wait().await.unwrap(), ChildExit::Terminated(15));
        assert_eq!(second.wait().await.unwrap(), ChildExit::Terminated(15));
    }

    #[tokio::test]
    async fn force_shuts_down_running_children() {
        let registry = create_registry();
        let first = registry.track(spawn_sleep()).await;
        let second = registry.track(spawn_sleep()).await;

        timeout(Duration::from_secs(5), registry.shutdown(SignalType::Kill))
            .await
            .unwrap();

        assert_eq!(registry.running_count(), 0);
        assert_eq!(first.wait().await.unwrap(), ChildExit::Killed);
        assert_eq!(second.wait().await.unwrap(), ChildExit::Killed);
    }

    #[tokio::test]
    async fn kill_terminates_descendants() {
        let registry = create_registry();
        let child = registry.track(spawn_piped_wrapper()).await;
        let stdout = wait_for_grandchild(&child).await;

        assert_eq!(
            registry.kill(&child).await.unwrap(),
            Some(ChildExit::Killed)
        );

        assert_pipe_closes(stdout).await;
    }

    #[tokio::test]
    async fn shutdown_terminates_descendants_of_every_child() {
        let registry = create_registry();
        let first = registry.track(spawn_piped_wrapper()).await;
        let second = registry.track(spawn_piped_wrapper()).await;
        let first_stdout = wait_for_grandchild(&first).await;
        let second_stdout = wait_for_grandchild(&second).await;

        timeout(Duration::from_secs(5), registry.shutdown(SignalType::Kill))
            .await
            .unwrap();

        assert_pipe_closes(first_stdout).await;
        assert_pipe_closes(second_stdout).await;
    }

    #[tokio::test]
    async fn dropping_the_registry_kills_descendants() {
        let registry = create_registry();
        let child = registry.track(spawn_piped_wrapper()).await;
        let stdout = wait_for_grandchild(&child).await;

        drop(registry);

        assert_eq!(child.wait().await.unwrap(), ChildExit::Killed);

        assert_pipe_closes(stdout).await;
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
    async fn exec_cached_recovers_from_a_cancelled_run() {
        let registry = Arc::new(create_registry());

        // A run that is dropped part way through
        let cancelled = timeout(
            Duration::from_millis(50),
            registry.exec_cached("key", std::future::pending::<Result<Output, ()>>()),
        )
        .await;

        assert!(cancelled.is_err());

        // Later calls must still share a single run
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

// Never called, as compiling it is the test. The registry guards its
// children with a synchronous lock, and a guard held across an await would
// make a future unable to move between threads, which nothing else in the
// crate would notice for the methods that it doesn't spawn itself.
#[allow(dead_code)]
fn public_futures_are_send(
    registry: &ProcessRegistry,
    command: &mut Command,
    child: Child,
    shared: &SharedChild,
) {
    fn assert_send<T: Send>(_future: T) {}

    assert_send(registry.spawn(command));
    assert_send(registry.track(child));
    assert_send(registry.release(shared));
    assert_send(registry.get(shared));
    assert_send(registry.get_running(shared));
    assert_send(registry.list());
    assert_send(registry.list_running());
    assert_send(registry.wait_for_idle());
    assert_send(registry.signal(shared, SignalType::Kill));
    assert_send(registry.signal_running(SignalType::Kill));
    assert_send(registry.kill(shared));
    assert_send(registry.shutdown(SignalType::Kill));
    assert_send(registry.get_cached_output("key"));
    assert_send(registry.cache_output("key", fake_output()));
    assert_send(registry.remove_cached_output("key"));
    assert_send(registry.clear_cache());
    assert_send(registry.exec_cached("key", async { Ok::<_, ()>(fake_output()) }));
}

fn fake_output() -> Output {
    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;
    #[cfg(windows)]
    use std::os::windows::process::ExitStatusExt;
    use std::process::ExitStatus;

    Output {
        exit: ChildExit::Completed(ExitStatus::from_raw(0)),
        stdout: "out".into(),
        stderr: "err".into(),
    }
}
