use starbase_process::{ChildExit, SharedChild, SignalType};
use std::process::Stdio;
use tokio::process::Command;

fn completed_child() -> SharedChild {
    #[cfg(unix)]
    let mut command = {
        let mut command = Command::new("sh");
        command.args(["-c", "exit 7"]);
        command
    };
    #[cfg(windows)]
    let mut command = {
        let mut command = Command::new("cmd.exe");
        command.args(["/D", "/C", "exit 7"]);
        command
    };
    SharedChild::new(command.stdout(Stdio::piped()).spawn().unwrap())
}

async fn assert_signalling_reaped_child(capture: bool) {
    let child = completed_child();
    let original = if capture {
        child.wait_with_output().await.unwrap().exit
    } else {
        child.wait().await.unwrap()
    };
    assert!(matches!(original, ChildExit::Completed(status) if status.code() == Some(7)));

    for signal in [SignalType::Terminate, SignalType::Kill] {
        assert_eq!(
            child.clone().kill_with_signal(signal).await.unwrap(),
            original
        );
        assert_eq!(child.wait().await.unwrap(), original);
    }

    assert_eq!(child.clone().kill().await.unwrap(), original);
    assert_eq!(child.wait().await.unwrap(), original);
}

#[tokio::test]
async fn signalling_after_wait_preserves_reaped_status() {
    assert_signalling_reaped_child(false).await;
}

#[tokio::test]
async fn signalling_after_capture_preserves_reaped_status() {
    assert_signalling_reaped_child(true).await;
}

#[cfg(unix)]
#[tokio::test]
async fn signalling_after_unreaped_normal_exit_preserves_status() {
    let child = SharedChild::new(
        Command::new("sh")
            .args(["-c", "sleep 0.05; exit 7"])
            .spawn()
            .unwrap(),
    );

    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    assert!(matches!(
        child.kill_with_signal(SignalType::Terminate).await.unwrap(),
        ChildExit::Completed(status) if status.code() == Some(7)
    ));
}
