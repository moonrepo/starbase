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

mod wait {
    use super::*;

    #[tokio::test]
    async fn returns_the_completed_status() {
        let child = SharedChild::new(Command::new("sh").args(["-c", "exit 3"]).spawn().unwrap());

        let ChildExit::Completed(status) = child.wait().await.unwrap() else {
            panic!("expected Completed");
        };

        assert_eq!(status.code(), Some(3));
        assert!(!status.success());
    }

    #[tokio::test]
    async fn returns_a_successful_status() {
        let child = SharedChild::new(Command::new("true").spawn().unwrap());

        let ChildExit::Completed(status) = child.wait().await.unwrap() else {
            panic!("expected Completed");
        };

        assert!(status.success());
    }

    #[tokio::test]
    async fn converts_external_signals() {
        for (signal, expected) in [
            ("INT", ChildExit::Interrupted),
            ("KILL", ChildExit::Killed),
            ("TERM", ChildExit::Terminated),
            // Anything that isn't an interrupt or kill is a termination
            ("QUIT", ChildExit::Terminated),
        ] {
            // The child signals itself, so no signal is recorded on the
            // shared child and the wait status is the only source
            let child = SharedChild::new(
                Command::new("sh")
                    .args(["-c", &format!("kill -{signal} $$; sleep 30")])
                    .spawn()
                    .unwrap(),
            );

            assert_eq!(child.wait().await.unwrap(), expected, "signal {signal}");
        }
    }

    #[tokio::test]
    async fn is_idempotent_once_the_child_has_exited() {
        let child = SharedChild::new(Command::new("sh").args(["-c", "exit 7"]).spawn().unwrap());

        let first = child.wait().await.unwrap();
        let second = child.wait().await.unwrap();

        assert_eq!(first, second);
        assert!(matches!(first, ChildExit::Completed(status) if status.code() == Some(7)));
    }

    #[tokio::test]
    async fn clones_observe_the_same_exit() {
        let child = SharedChild::new(Command::new("sh").args(["-c", "exit 4"]).spawn().unwrap());
        let clone = child.clone();

        assert_eq!(child.wait().await.unwrap(), clone.wait().await.unwrap());
    }
}

mod wait_with_output {
    use super::*;

    fn spawn_printing(script: &str) -> SharedChild {
        SharedChild::new(
            Command::new("sh")
                .args(["-c", script])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        )
    }

    #[tokio::test]
    async fn captures_both_pipes_and_the_exit() {
        let output = spawn_printing("printf 'out'; printf 'err' 1>&2")
            .wait_with_output()
            .await
            .unwrap();

        assert!(output.success());
        assert_eq!(output.stdout.as_ref(), b"out");
        assert_eq!(output.stderr.as_ref(), b"err");
    }

    #[tokio::test]
    async fn captures_a_nonzero_exit() {
        let output = spawn_printing("printf 'boom' 1>&2; exit 2")
            .wait_with_output()
            .await
            .unwrap();

        assert!(!output.success());
        assert_eq!(output.code(), Some(2));
        assert_eq!(output.stderr.as_ref(), b"boom");
    }

    #[tokio::test]
    async fn preserves_non_utf8_bytes() {
        let output = spawn_printing(r"printf 'a\377b'")
            .wait_with_output()
            .await
            .unwrap();

        assert_eq!(output.stdout.as_ref(), b"a\xffb");
    }

    #[tokio::test]
    async fn returns_empty_bytes_without_pipes() {
        let child = SharedChild::new(
            Command::new("sh")
                .args(["-c", "printf 'inherited'"])
                .stdout(Stdio::null())
                .spawn()
                .unwrap(),
        );

        let output = child.wait_with_output().await.unwrap();

        assert!(output.success());
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }

    #[tokio::test]
    async fn returns_empty_bytes_for_pipes_taken_elsewhere() {
        let child = spawn_printing("printf 'out'; printf 'err' 1>&2");

        // Hold the pipe open for the life of the test, as dropping it
        // would close the read end and kill the child with SIGPIPE
        let stdout = child.take_stdout().await;

        assert!(stdout.is_some());

        let output = child.wait_with_output().await.unwrap();

        assert!(output.stdout.is_empty());
        assert_eq!(output.stderr.as_ref(), b"err");
    }

    #[tokio::test]
    async fn converts_external_signals() {
        let output = spawn_printing("printf 'out'; kill -9 $$")
            .wait_with_output()
            .await
            .unwrap();

        assert_eq!(output.exit, ChildExit::Killed);
        assert_eq!(output.code(), None);
        assert_eq!(output.stdout.as_ref(), b"out");
    }

    #[tokio::test]
    async fn a_clone_can_be_killed_while_waiting() {
        // This is why the method borrows instead of taking ownership:
        // the signal is sent before the child lock is acquired, so a
        // kill lands even though a waiter may be holding that lock
        let marker = std::env::temp_dir().join(format!(
            "starbase-process-wait-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_file(&marker);

        // `exec` so that the process we signal is the one holding the
        // stdout pipe, otherwise killing the shell orphans its child and
        // the pipe never reaches EOF
        let child = spawn_printing(&format!(
            "printf 'out'; touch '{}'; exec sleep 30",
            marker.display()
        ));
        let waiter = child.clone();

        let handle = tokio::spawn(async move { waiter.wait_with_output().await });

        // Signalling before the child writes would race the output away,
        // so wait for it to tell us it got that far
        for _ in 0..1000 {
            if marker.exists() {
                break;
            }

            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        assert!(marker.exists(), "child never started");

        assert_eq!(
            child.kill_with_signal(SignalType::Terminate).await.unwrap(),
            ChildExit::Terminated
        );

        let output = handle.await.unwrap().unwrap();

        assert_eq!(output.exit, ChildExit::Terminated);
        assert_eq!(output.stdout.as_ref(), b"out");

        let _ = std::fs::remove_file(&marker);
    }
}
