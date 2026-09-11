#![cfg(windows)]

use starbase_process::{ChildExit, SharedChild};
use tokio::process::Command;
use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
use windows_sys::Win32::System::Threading::WaitForSingleObject;

#[tokio::test]
async fn kill_after_exit_before_reaping_does_not_poison_the_wait_status() {
    let child = Command::new("cmd")
        .args(["/D", "/S", "/C", "exit 0"])
        .spawn()
        .unwrap();
    let handle = child.raw_handle().unwrap();
    let child = SharedChild::new(child);

    assert_eq!(
        unsafe { WaitForSingleObject(handle.cast(), 5_000) },
        WAIT_OBJECT_0
    );

    let _ = child.kill().await;

    let ChildExit::Completed(status) = child.wait().await.unwrap() else {
        panic!("expected Completed");
    };

    assert!(status.success());
}
