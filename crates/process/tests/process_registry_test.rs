#![cfg(unix)]

use starbase_process::{ProcessRegistry, SignalType};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

fn spawn_sleep() -> Child {
    Command::new("sleep").arg("30").spawn().unwrap()
}

// The registry spawns its signal handling tasks onto the runtime that
// created it, so tests that exercise shutdown must own their registry.
// The singleton is created by whichever test runs first, and its tasks
// die with that test's runtime.
fn create_registry() -> ProcessRegistry {
    ProcessRegistry::new(2000)
}

mod process_registry {
    use super::*;

    #[tokio::test]
    async fn instance_is_a_singleton() {
        assert!(Arc::ptr_eq(
            &ProcessRegistry::instance(),
            &ProcessRegistry::instance()
        ));
    }

    #[tokio::test]
    async fn registers_and_unregisters_children() {
        let registry = create_registry();
        let shared = registry.add_running(spawn_sleep()).await;
        let pid = shared.id();

        assert!(registry.get_running_by_pid(pid).await.is_some());

        registry.remove_running(shared.clone()).await;

        assert!(registry.get_running_by_pid(pid).await.is_none());

        let _ = shared.kill().await;
    }

    #[tokio::test]
    async fn unregisters_children_by_pid() {
        let registry = create_registry();
        let shared = registry.add_running(spawn_sleep()).await;
        let pid = shared.id();

        registry.remove_running_by_pid(pid).await;

        assert!(registry.get_running_by_pid(pid).await.is_none());

        let _ = shared.kill().await;
    }

    #[tokio::test]
    async fn unknown_pids_are_not_running() {
        assert!(create_registry().get_running_by_pid(0).await.is_none());
    }

    #[tokio::test]
    async fn terminates_running_children() {
        let registry = create_registry();
        let shared = registry.add_running(spawn_sleep()).await;
        let pid = shared.id();

        registry.terminate_running();
        registry.wait_for_running_to_shutdown().await;

        assert!(registry.get_running_by_pid(pid).await.is_none());
    }

    #[tokio::test]
    async fn a_signal_received_with_no_children_does_not_stop_shutdown_handling() {
        let registry = create_registry();

        registry.terminate_running();
        tokio::task::yield_now().await;

        let shared = registry.add_running(spawn_sleep()).await;
        let pid = shared.id();

        registry.terminate_running();
        registry.wait_for_running_to_shutdown().await;

        assert!(registry.get_running_by_pid(pid).await.is_none());
        assert_eq!(
            shared.wait().await.unwrap(),
            starbase_process::ChildExit::Terminated(15)
        );
    }

    #[tokio::test]
    async fn shutdown_handling_continues_for_later_children() {
        let registry = create_registry();

        let first = registry.add_running(spawn_sleep()).await;
        registry.terminate_running();
        registry.wait_for_running_to_shutdown().await;
        assert_eq!(
            first.wait().await.unwrap(),
            starbase_process::ChildExit::Terminated(15)
        );

        let second = registry.add_running(spawn_sleep()).await;
        registry.terminate_running();
        registry.wait_for_running_to_shutdown().await;
        assert_eq!(
            second.wait().await.unwrap(),
            starbase_process::ChildExit::Terminated(15)
        );
    }

    #[tokio::test]
    async fn dropping_registry_kills_tracked_children() {
        let registry = create_registry();
        let child = registry.add_running(spawn_sleep()).await;

        drop(registry);

        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(1), child.wait())
                .await
                .unwrap()
                .unwrap(),
            starbase_process::ChildExit::Killed
        );
    }

    #[tokio::test]
    async fn zero_threshold_still_signals_before_waiting() {
        let registry = ProcessRegistry::new(0);
        let child = registry.add_running(spawn_sleep()).await;

        registry.terminate_running();

        assert_eq!(
            tokio::time::timeout(
                std::time::Duration::from_secs(1),
                registry.wait_for_running_to_shutdown()
            )
            .await
            .unwrap(),
            ()
        );
        assert_eq!(
            child.wait().await.unwrap(),
            starbase_process::ChildExit::Terminated(15)
        );
    }

    #[tokio::test]
    async fn a_second_signal_force_kills_without_waiting_for_the_threshold() {
        let registry = ProcessRegistry::new(5000);
        let child = registry
            .add_running(
                Command::new("sh")
                    .args(["-c", "trap '' TERM; echo ready; exec sleep 30"])
                    .stdout(Stdio::piped())
                    .spawn()
                    .unwrap(),
            )
            .await;

        let mut ready = String::new();
        BufReader::new(child.take_stdout().await.unwrap())
            .read_line(&mut ready)
            .await
            .unwrap();
        assert_eq!(ready, "ready\n");

        registry.terminate_running();
        registry.terminate_running();

        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), child.wait())
                .await
                .expect("second signal did not bypass the shutdown threshold")
                .unwrap(),
            starbase_process::ChildExit::Killed
        );
    }

    #[tokio::test]
    async fn shutdown_wait_returns_immediately_when_empty() {
        create_registry().wait_for_running_to_shutdown().await;
    }

    #[tokio::test]
    async fn broadcasts_signals_to_receivers() {
        let registry = create_registry();
        let mut first = registry.receive_signal();
        let mut second = registry.receive_signal();

        registry.terminate_running();

        assert!(matches!(first.recv().await.unwrap(), SignalType::Terminate));
        assert!(matches!(
            second.recv().await.unwrap(),
            SignalType::Terminate
        ));
    }
}
