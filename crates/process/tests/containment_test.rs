#![cfg(unix)]

use starbase_console::{Console, EmptyReporter};
use starbase_process::{ChildExit, Command, ProcessSet, SharedChild, SignalType};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command as TokioCommand;
use tokio::time::{sleep, timeout};

// A process the test started through a shell, killed when the test ends
// in case it's still running
struct Descendant(u32);

impl Descendant {
    fn is_alive(&self) -> bool {
        unsafe { libc::kill(self.0 as i32, 0) == 0 }
    }

    // Once its parent is gone, the process is reaped by init
    async fn wait_until_gone(&self) {
        timeout(Duration::from_secs(3), async {
            while self.is_alive() {
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("descendant was not killed");
    }
}

impl Drop for Descendant {
    fn drop(&mut self) {
        let _ = starbase_process::kill(self.0, SignalType::Kill);
    }
}

// Spawn a shell running `script`, which prints the pid of a process it
// starts in the background as its first line
async fn spawn_shell(set: &ProcessSet, script: &str) -> (SharedChild, Descendant) {
    let child = set
        .spawn_contained(
            TokioCommand::new("sh")
                .args(["-c", script])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped()),
        )
        .unwrap();

    let mut pid = String::new();
    timeout(
        Duration::from_secs(3),
        BufReader::new(child.take_stdout().await.unwrap()).read_line(&mut pid),
    )
    .await
    .unwrap()
    .unwrap();

    (child, Descendant(pid.trim().parse().unwrap()))
}

#[tokio::test]
async fn contained_children_lead_their_own_process_group() {
    let set = ProcessSet::root().nested();
    let contained = set
        .spawn_contained(TokioCommand::new("sleep").arg("30"))
        .unwrap();
    let uncontained = set.spawn(TokioCommand::new("sleep").arg("30")).unwrap();

    assert!(contained.is_contained());
    assert!(!uncontained.is_contained());
    assert_eq!(
        unsafe { libc::getpgid(contained.id() as i32) },
        contained.id() as i32
    );
    assert_eq!(unsafe { libc::getpgid(uncontained.id() as i32) }, unsafe {
        libc::getpgrp()
    });
}

#[tokio::test]
async fn shutdowns_reach_the_whole_tree() {
    let set = ProcessSet::root().nested_with_grace(Duration::from_secs(5));
    let (child, descendant) = spawn_shell(&set, "sleep 30 & echo $!; wait").await;

    // The shutdown waits out its grace period, so run it in the background
    tokio::spawn({
        let set = set.clone();

        async move { set.shutdown(SignalType::Terminate).await }
    });

    assert_eq!(
        timeout(Duration::from_secs(3), child.wait())
            .await
            .unwrap()
            .unwrap(),
        ChildExit::Terminated(SignalType::Terminate.get_code())
    );

    descendant.wait_until_gone().await;
}

#[tokio::test]
async fn dropping_a_running_owner_kills_the_whole_tree() {
    let set = ProcessSet::root().nested();
    let (child, descendant) = spawn_shell(&set, "sleep 30 & echo $!; wait").await;

    drop(child);

    descendant.wait_until_gone().await;
}

#[tokio::test]
async fn processes_left_behind_after_a_normal_exit_keep_running() {
    let set = ProcessSet::root().nested();
    let (child, descendant) = spawn_shell(&set, "sleep 30 >/dev/null 2>&1 & echo $!").await;

    child.wait().await.unwrap();
    drop(child);
    sleep(Duration::from_millis(100)).await;

    assert!(descendant.is_alive());
}

#[tokio::test]
async fn signals_reach_processes_left_behind_by_an_exited_child() {
    let set = ProcessSet::root().nested_with_grace(Duration::from_secs(10));
    // The background process keeps stderr open after the shell exits
    let (child, descendant) = spawn_shell(&set, "sleep 30 & echo $!; exit 7").await;

    let exit = child.wait().await.unwrap();
    let capture = child.wait_with_output();
    tokio::pin!(capture);

    assert!(
        timeout(Duration::from_millis(50), capture.as_mut())
            .await
            .is_err()
    );

    // The shutdown waits out its grace period, but the capture completes
    // as soon as the signal closes the pipes, without being stopped
    tokio::spawn({
        let set = set.clone();

        async move { set.shutdown(SignalType::Terminate).await }
    });

    let output = timeout(Duration::from_secs(1), capture)
        .await
        .expect("signal did not reach the process holding the pipes")
        .unwrap();

    assert_eq!(output.exit, exit);
    assert!(matches!(exit, ChildExit::Completed(status) if status.code() == Some(7)));

    descendant.wait_until_gone().await;
}

#[tokio::test]
async fn commands_contain_their_tree_by_default() {
    let mut command = Command::<EmptyReporter>::new("sh");
    command
        .no_shell()
        .args(["-c", "echo $$; ps -o pgid= -p $$"])
        .set_console(Console::new_testing());

    let output = command.exec_capture_output().await.unwrap();
    let stdout = starbase_process::output_to_string(&output.stdout);
    let lines = stdout.lines().map(str::trim).collect::<Vec<_>>();

    // The shell leads its own process group
    assert_eq!(lines[0], lines[1]);

    command.set_contain_tree(false);

    let output = command.exec_capture_output().await.unwrap();
    let stdout = starbase_process::output_to_string(&output.stdout);
    let lines = stdout.lines().map(str::trim).collect::<Vec<_>>();

    assert_eq!(lines[1], unsafe { libc::getpgrp() }.to_string());
}
