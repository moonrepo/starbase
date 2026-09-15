use starbase_console::EmptyReporter;
use starbase_process::process_registry_new::{
    ProcessRegistry, RegistryEvent, RegistryOptions, RegistryState,
};
use starbase_process::{Command, SignalType};
use std::future::Future;
use std::time::Duration;

fn registry() -> ProcessRegistry {
    ProcessRegistry::with_options(RegistryOptions {
        shutdown_grace: Duration::from_millis(100),
        output_drain_timeout: Duration::from_millis(200),
        ..RegistryOptions::default()
    })
}

async fn bounded<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(10), future)
        .await
        .expect("operation timed out")
}

// Re-execute this integration test binary as a portable subprocess fixture.
// No shell or external runtime is required for the platform-independent tests.
fn fixture(mode: &str) -> Command<EmptyReporter> {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .no_shell()
        .args(["--exact", "child_fixture", "--nocapture"])
        .env("STARBASE_NEW_FIXTURE", mode);
    command
}

#[test]
fn child_fixture() {
    use std::io::{Read, Write};
    match std::env::var("STARBASE_NEW_FIXTURE").as_deref() {
        Ok("echo") => {
            let mut input = Vec::new();
            std::io::stdin().read_to_end(&mut input).unwrap();
            std::io::stdout().write_all(&input).unwrap();
            std::io::stderr().write_all(b"fixture stderr").unwrap();
        }
        Ok("duplex") => {
            // Fill both output pipes before reading an input larger than a pipe.
            std::io::stdout()
                .write_all(&vec![b'x'; 256 * 1024])
                .unwrap();
            std::io::stderr()
                .write_all(&vec![b'y'; 256 * 1024])
                .unwrap();
            let mut input = Vec::new();
            std::io::stdin().read_to_end(&mut input).unwrap();
            assert_eq!(input.len(), 256 * 1024);
        }
        Ok("sleep") => std::thread::sleep(Duration::from_secs(30)),
        #[cfg(unix)]
        Ok("signals") => {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    let registry = registry();
                    registry.listen_for_signals().unwrap();
                    registry.listen_for_signals().unwrap();
                    let child = registry.spawn(&fixture("sleep")).await.unwrap();
                    let mut events = registry.subscribe();
                    println!("listener ready");
                    std::io::stdout().flush().unwrap();
                    bounded(async {
                        loop {
                            if matches!(
                                events.recv().await.unwrap(),
                                RegistryEvent::StateChanged(RegistryState::Stopped)
                            ) {
                                break;
                            }
                        }
                    })
                    .await;
                    assert!(registry.running().is_empty());
                    assert_reaped(child.pid());
                    registry.stop_signal_listener();
                    registry.start().unwrap();
                    registry.listen_for_signals().unwrap();
                    registry.stop_signal_listener();
                });
        }
        Ok("fail") => std::process::exit(7),
        Ok("env") => println!(
            "VALUE={}",
            std::env::var("STARBASE_VALUE").unwrap_or_default()
        ),
        _ => {}
    }
}

fn assert_reaped(pid: u32) {
    #[cfg(unix)]
    {
        let mut status = 0;
        // The registry must already have collected the direct child's status.
        assert_eq!(
            unsafe { libc::waitpid(pid as i32, &mut status, libc::WNOHANG) },
            -1
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ECHILD)
        );
    }
    #[cfg(windows)]
    let _ = pid;
}

#[tokio::test]
async fn captures_input_and_both_outputs_then_reaps() {
    let registry = registry();
    let mut command = fixture("echo");
    command.input(["hello", "world"]);
    let mut events = registry.subscribe();
    let child = registry.spawn(&command).await.unwrap();
    let output = bounded(child.wait()).await.unwrap();
    assert!(output.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("hello world"));
    assert_eq!(&output.stderr[..], b"fixture stderr");
    assert!(registry.running().is_empty());
    assert_reaped(child.pid());
    assert!(matches!(
        events.recv().await.unwrap(),
        RegistryEvent::Spawned { .. }
    ));
    assert!(matches!(
        events.recv().await.unwrap(),
        RegistryEvent::Completed { .. }
    ));
}

#[tokio::test]
async fn input_and_output_larger_than_pipe_capacity_do_not_deadlock() {
    let registry = registry();
    let mut command = fixture("duplex");
    command.input(["z".repeat(256 * 1024)]);
    let output = bounded(registry.execute(&mut command)).await.unwrap();
    assert!(output.stdout.len() >= 256 * 1024);
    assert_eq!(output.stderr.len(), 256 * 1024);
}

#[tokio::test]
async fn closes_empty_stdin_and_preserves_continuous_input_semantics() {
    let registry = registry();
    bounded(registry.execute(&mut fixture("echo")))
        .await
        .unwrap();
    let mut command = fixture("echo");
    command.set_continuous_pipe(true).input(["one", "two"]);
    let output = bounded(registry.execute(&mut command)).await.unwrap();
    assert!(String::from_utf8_lossy(&output.stdout).contains("onetwo"));
}

#[tokio::test]
async fn cancelled_wait_can_be_resumed_by_multiple_callers() {
    let registry = registry();
    let child = registry.spawn(&fixture("sleep")).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(10), child.wait())
            .await
            .is_err()
    );
    child.signal(SignalType::Kill).unwrap();
    let (first, second) = bounded(async { tokio::join!(child.wait(), child.wait()) }).await;
    assert_eq!(first.unwrap(), second.unwrap());
    assert_reaped(child.pid());
    child.signal(SignalType::Kill).unwrap();
}

#[tokio::test]
async fn discarded_handle_still_has_a_supervisor() {
    let registry = registry();
    let child = registry.spawn(&fixture("sleep")).await.unwrap();
    let pid = child.pid();
    drop(child);
    assert_eq!(registry.running().len(), 1);
    bounded(registry.shutdown()).await.unwrap();
    assert!(registry.running().is_empty());
    assert_reaped(pid);
}

#[tokio::test]
async fn stopping_resuming_and_shutdown_preserve_state() {
    let registry = registry();
    let child = registry.spawn(&fixture("sleep")).await.unwrap();
    registry.stop().unwrap();
    assert_eq!(registry.state(), RegistryState::Stopped);
    assert!(registry.spawn(&fixture("echo")).await.is_err());
    assert_eq!(registry.running()[0].id(), child.id());
    registry.restart().unwrap();
    bounded(registry.execute(&mut fixture("echo")))
        .await
        .unwrap();
    bounded(registry.shutdown()).await.unwrap();
    assert_eq!(registry.state(), RegistryState::Stopped);
    assert!(registry.running().is_empty());
    registry.start().unwrap();
    bounded(registry.execute(&mut fixture("echo")))
        .await
        .unwrap();
}

#[tokio::test]
async fn registries_are_independent_and_last_owner_drop_kills() {
    let first = registry();
    let second = registry();
    let first_child = first.spawn(&fixture("sleep")).await.unwrap();
    let second_child = second.spawn(&fixture("sleep")).await.unwrap();
    let clone = first.clone();
    drop(first);
    assert!(
        tokio::time::timeout(Duration::from_millis(10), first_child.wait())
            .await
            .is_err()
    );
    drop(clone);
    bounded(first_child.wait()).await.unwrap();
    assert_eq!(second.running().len(), 1);
    bounded(second.shutdown()).await.unwrap();
    assert_reaped(second_child.pid());
}

#[tokio::test]
async fn shutdown_continues_after_the_waiter_is_cancelled() {
    let registry = registry();
    let child = registry.spawn(&fixture("sleep")).await.unwrap();
    // Poll shutdown once without depending on how quickly this OS exits a child.
    let mut shutdown = Box::pin(registry.shutdown());
    std::future::poll_fn(|cx| {
        let _ = shutdown.as_mut().poll(cx);
        std::task::Poll::Ready(())
    })
    .await;
    drop(shutdown);
    bounded(child.wait()).await.unwrap();
    bounded(registry.shutdown()).await.unwrap();
    assert_eq!(registry.state(), RegistryState::Stopped);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_spawn_and_shutdown_leave_no_unmanaged_children() {
    let registry = registry();
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..20 {
        let registry = registry.clone();
        tasks.spawn(async move { registry.spawn(&fixture("sleep")).await });
    }
    tokio::task::yield_now().await;
    let (first, second) =
        bounded(async { tokio::join!(registry.shutdown(), registry.shutdown()) }).await;
    first.unwrap();
    second.unwrap();
    while let Some(result) = tasks.join_next().await {
        if let Ok(child) = result.unwrap() {
            bounded(child.wait()).await.unwrap();
            assert_reaped(child.pid());
        }
    }
    assert!(registry.running().is_empty());
}

#[tokio::test]
async fn output_limit_reports_failure_and_reaps() {
    let registry = ProcessRegistry::with_options(RegistryOptions {
        max_output_bytes: 1024,
        ..RegistryOptions::default()
    });
    let child = registry.spawn(&fixture("duplex")).await.unwrap();
    let error = bounded(child.wait()).await.unwrap_err();
    assert!(error.to_string().contains("byte limit"));
    assert!(registry.running().is_empty());
    assert_reaped(child.pid());
}

#[tokio::test]
async fn failed_spawn_and_nonzero_exit_do_not_poison_registry() {
    let registry = registry();
    let mut missing = Command::<EmptyReporter>::new("/does/not/exist/starbase-new");
    missing.no_shell();
    assert!(registry.spawn(&missing).await.is_err());
    assert!(registry.running().is_empty());
    assert!(
        bounded(registry.execute(&mut fixture("fail")))
            .await
            .is_err()
    );
    let mut command = fixture("fail");
    command.set_error_on_nonzero(false);
    assert_eq!(
        bounded(registry.execute(&mut command))
            .await
            .unwrap()
            .code(),
        Some(7)
    );
    bounded(registry.execute(&mut fixture("echo")))
        .await
        .unwrap();
}

fn spawned_count(events: &mut tokio::sync::broadcast::Receiver<RegistryEvent>) -> usize {
    let mut count = 0;
    while let Ok(event) = events.try_recv() {
        if matches!(event, RegistryEvent::Spawned { .. }) {
            count += 1;
        }
    }
    count
}

#[tokio::test]
async fn cache_survives_shutdown_and_observes_input_and_environment() {
    let registry = registry();
    let mut events = registry.subscribe();
    let mut command = fixture("env");
    command.set_cache(true).env("STARBASE_VALUE", "first");
    let first = bounded(registry.execute(&mut command)).await.unwrap();
    bounded(registry.shutdown()).await.unwrap();
    assert!(registry.execute(&mut command).await.is_err());
    registry.start().unwrap();
    assert_eq!(
        bounded(registry.execute(&mut command)).await.unwrap(),
        first
    );
    assert_eq!(spawned_count(&mut events), 1);
    command.env("STARBASE_VALUE", "second");
    let second = bounded(registry.execute(&mut command)).await.unwrap();
    assert_ne!(first.stdout, second.stdout);
    assert_eq!(spawned_count(&mut events), 1);
    registry.clear_cache();
    bounded(registry.execute(&mut command)).await.unwrap();
    assert_eq!(spawned_count(&mut events), 1);
}

#[tokio::test]
async fn cache_has_capacity_ttl_and_never_stores_failures() {
    let registry = ProcessRegistry::with_options(RegistryOptions {
        cache_capacity: 1,
        cache_ttl: Duration::from_millis(200),
        ..RegistryOptions::default()
    });
    let mut events = registry.subscribe();
    let mut first = fixture("env");
    first.set_cache(true).env("STARBASE_VALUE", "first");
    let mut second = fixture("env");
    second.set_cache(true).env("STARBASE_VALUE", "second");
    bounded(registry.execute(&mut first)).await.unwrap();
    bounded(registry.execute(&mut second)).await.unwrap();
    bounded(registry.execute(&mut first)).await.unwrap();
    assert_eq!(spawned_count(&mut events), 3);
    tokio::time::sleep(Duration::from_millis(220)).await;
    bounded(registry.execute(&mut first)).await.unwrap();
    assert_eq!(spawned_count(&mut events), 1);
    let mut failing = fixture("fail");
    failing.set_cache(true).set_error_on_nonzero(false);
    for _ in 0..2 {
        bounded(registry.execute(&mut failing)).await.unwrap();
    }
    assert_eq!(spawned_count(&mut events), 2);
}

#[tokio::test]
async fn lagging_events_do_not_block_supervision() {
    let registry = registry();
    let _events = registry.subscribe();
    for _ in 0..150 {
        registry.stop().unwrap();
        registry.start().unwrap();
    }
    bounded(registry.execute(&mut fixture("echo")))
        .await
        .unwrap();
    assert!(registry.running().is_empty());
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct FixtureDir(PathBuf);
    impl FixtureDir {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "starbase-new-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        async fn read_ready(&self) -> String {
            bounded(async {
                loop {
                    if let Ok(value) = std::fs::read_to_string(self.0.join("ready"))
                        && !value.trim().is_empty()
                    {
                        return value.trim().to_owned();
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
        }
    }
    impl Drop for FixtureDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn shell(script: &str, dir: &FixtureDir) -> Command<EmptyReporter> {
        let mut command = Command::new("/bin/sh");
        command.no_shell().args(["-c", script]).cwd(&dir.0);
        command
    }

    #[tokio::test]
    async fn kills_grandchild_before_reaping_naturally_exited_leader() {
        let registry = registry();
        let dir = FixtureDir::new();
        let command = shell("sleep 30 & echo $! > ready; printf complete", &dir);
        let child = registry.spawn(&command).await.unwrap();
        let descendant: i32 = dir.read_ready().await.parse().unwrap();
        let output = bounded(child.wait()).await.unwrap();
        assert_eq!(&output.stdout[..], b"complete");
        assert_reaped(child.pid());
        // An orphan can briefly remain a zombie until init reaps it. A killed
        // process must stop holding our pipe, which the successful capture proves.
        let status = std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", &descendant.to_string()])
            .output()
            .unwrap();
        let state = String::from_utf8_lossy(&status.stdout);
        assert!(
            state.trim().is_empty() || state.contains('Z'),
            "descendant still running: {state}"
        );
    }

    #[tokio::test]
    async fn shutdown_escalates_when_process_ignores_termination() {
        let registry = registry();
        let dir = FixtureDir::new();
        let child = registry
            .spawn(&shell(
                "trap '' TERM; echo ready > ready; while :; do sleep 1; done",
                &dir,
            ))
            .await
            .unwrap();
        dir.read_ready().await;
        let start = std::time::Instant::now();
        bounded(registry.shutdown()).await.unwrap();
        assert!(start.elapsed() >= Duration::from_millis(90));
        assert!(matches!(
            child.wait().await.unwrap().exit,
            starbase_process::ChildExit::Killed
        ));
        assert_reaped(child.pid());
    }

    #[tokio::test]
    async fn explicit_interrupt_reaches_the_process_group() {
        let registry = registry();
        let dir = FixtureDir::new();
        let child = registry
            .spawn(&shell(
                "trap 'exit 23' INT; echo ready > ready; while :; do sleep 1; done",
                &dir,
            ))
            .await
            .unwrap();
        dir.read_ready().await;
        child.signal(SignalType::Interrupt).unwrap();
        assert_eq!(bounded(child.wait()).await.unwrap().code(), Some(23));
    }

    #[tokio::test]
    async fn cache_keys_distinguish_input_and_working_directory() {
        let registry = registry();
        let first = FixtureDir::new();
        let second = FixtureDir::new();
        let mut command = shell("pwd; cat", &first);
        command.set_cache(true).input(["first"]);
        let a = bounded(registry.execute(&mut command)).await.unwrap();
        command.input = vec!["second".into()];
        let b = bounded(registry.execute(&mut command)).await.unwrap();
        assert_ne!(a.stdout, b.stdout);
        command.cwd(&second.0);
        let c = bounded(registry.execute(&mut command)).await.unwrap();
        assert_ne!(b.stdout, c.stdout);
    }

    #[tokio::test]
    async fn clearing_cache_invalidates_inflight_results() {
        let registry = registry();
        let dir = FixtureDir::new();
        let mut command = shell("echo ready > ready; sleep 0.2; printf result", &dir);
        command.set_cache(true);
        let mut events = registry.subscribe();
        let task_registry = registry.clone();
        let task = tokio::spawn(async move { task_registry.execute(&mut command).await });
        dir.read_ready().await;
        registry.clear_cache();
        bounded(task).await.unwrap().unwrap();
        let mut command = shell("echo ready > ready; sleep 0.2; printf result", &dir);
        command.set_cache(true);
        bounded(registry.execute(&mut command)).await.unwrap();
        assert_eq!(spawned_count(&mut events), 2);
    }
}

#[cfg(unix)]
#[tokio::test]
async fn os_signal_listener_shuts_down_in_an_isolated_process() {
    use std::process::Stdio;
    use tokio::io::{AsyncBufReadExt, BufReader};
    let mut command = fixture("signals").create_async_command().unwrap();
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let pid = child.id().unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    bounded(async {
        while let Some(line) = lines.next_line().await.unwrap() {
            if line == "listener ready" {
                return;
            }
        }
        panic!("fixture exited before installing its listener");
    })
    .await;
    // Signal only the isolated fixture, never this test runner or its group.
    assert_eq!(unsafe { libc::kill(pid as i32, libc::SIGTERM) }, 0);
    let output = bounded(child.wait_with_output()).await.unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn cache_byte_budget_excludes_oversized_outputs() {
    let registry = ProcessRegistry::with_options(RegistryOptions {
        cache_max_bytes: 1,
        ..RegistryOptions::default()
    });
    let mut command = fixture("echo");
    command.set_cache(true);
    let mut events = registry.subscribe();
    for _ in 0..2 {
        bounded(registry.execute(&mut command)).await.unwrap();
    }
    assert_eq!(spawned_count(&mut events), 2);
}
