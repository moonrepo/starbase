#![cfg(unix)]

use starbase_process::{ChildExit, SharedChild, SignalType};
use std::process::Stdio;
use tokio::process::Command;

fn spawn_sleep() -> SharedChild {
    SharedChild::new(Command::new("sleep").arg("30").spawn().unwrap())
}

mod shared_child {
    use super::*;

    #[tokio::test]
    async fn returns_a_pid() {
        let child = spawn_sleep();

        assert!(child.id() > 0);

        let _ = child.kill().await;
    }

    #[tokio::test]
    async fn takes_pipes_only_once() {
        let mut command = Command::new("sleep");
        command.arg("30").stdout(Stdio::piped());

        let child = SharedChild::new(command.spawn().unwrap());

        assert!(child.take_stdout().await.is_some());
        assert!(child.take_stdout().await.is_none());
        assert!(child.take_stderr().await.is_none());

        let _ = child.kill().await;
    }

    #[tokio::test]
    async fn takes_stdin_only_once() {
        let mut command = Command::new("cat");
        command.stdin(Stdio::piped());

        let child = SharedChild::new(command.spawn().unwrap());

        assert!(child.take_stdin().await.is_some());
        assert!(child.take_stdin().await.is_none());

        let _ = child.kill().await;
    }

    #[tokio::test]
    async fn takes_stderr_when_piped() {
        let mut command = Command::new("sleep");
        command.arg("30").stderr(Stdio::piped());

        let child = SharedChild::new(command.spawn().unwrap());

        assert!(child.take_stderr().await.is_some());
        assert!(child.take_stderr().await.is_none());

        let _ = child.kill().await;
    }

    #[tokio::test]
    async fn kill_reports_killed() {
        assert_eq!(spawn_sleep().kill().await.unwrap(), ChildExit::Killed);
    }

    #[tokio::test]
    async fn interrupt_signal_reports_interrupted() {
        assert_eq!(
            spawn_sleep()
                .kill_with_signal(SignalType::Interrupt)
                .await
                .unwrap(),
            ChildExit::Interrupted
        );
    }

    #[tokio::test]
    async fn kill_signal_reports_killed() {
        assert_eq!(
            spawn_sleep()
                .kill_with_signal(SignalType::Kill)
                .await
                .unwrap(),
            ChildExit::Killed
        );
    }

    #[tokio::test]
    async fn terminate_signal_reports_terminated() {
        assert_eq!(
            spawn_sleep()
                .kill_with_signal(SignalType::Terminate)
                .await
                .unwrap(),
            ChildExit::Terminated
        );
    }

    #[tokio::test]
    async fn quit_signal_reports_terminated() {
        // Only interrupt and kill have dedicated variants
        assert_eq!(
            spawn_sleep()
                .kill_with_signal(SignalType::Quit)
                .await
                .unwrap(),
            ChildExit::Terminated
        );
    }

    #[tokio::test]
    async fn signalling_twice_keeps_the_first_signal() {
        let child = spawn_sleep();

        assert_eq!(
            child.kill_with_signal(SignalType::Interrupt).await.unwrap(),
            ChildExit::Interrupted
        );
        assert_eq!(
            child.kill_with_signal(SignalType::Terminate).await.unwrap(),
            ChildExit::Interrupted
        );
    }

    #[tokio::test]
    async fn killing_an_exited_child_is_not_an_error() {
        let child = SharedChild::new(Command::new("true").spawn().unwrap());

        // Reap it first, so the signal lands on an already dead process
        assert!(child.kill_with_signal(SignalType::Terminate).await.is_ok());
        assert!(child.kill_with_signal(SignalType::Kill).await.is_ok());
    }

    #[tokio::test]
    async fn clones_share_the_same_child() {
        let child = spawn_sleep();
        let clone = child.clone();

        assert_eq!(child.id(), clone.id());
        assert_eq!(clone.kill().await.unwrap(), ChildExit::Killed);
    }
}
