#![cfg(unix)]

use starbase_process::{ProcessRegistry, SignalType};
use std::sync::Arc;
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
