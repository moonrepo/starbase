#![cfg(windows)]

use starbase_console::{Console, EmptyReporter};
use starbase_process::{CaptureOptions, Command, ProcessError};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

const HELPER_ENV: &str = "STARBASE_PROCESS_WINDOWS_HELPER";
const MARKER_ENV: &str = "STARBASE_PROCESS_WINDOWS_MARKER";

fn create_helper(mode: &str) -> Command<EmptyReporter> {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args(["--exact", "process_helper", "--nocapture"]);
    command.env(HELPER_ENV, mode);
    command.no_shell();
    command.set_console(Arc::new(Console::new_testing()));
    command
}

fn temp_path(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    std::env::temp_dir().join(format!(
        "starbase-process-{name}-{}-{nonce}",
        std::process::id()
    ))
}

#[test]
fn process_helper() {
    match std::env::var(HELPER_ENV).as_deref() {
        Ok("output") => {
            print!("captured-stdout");
            std::io::stdout().flush().unwrap();
            eprint!("captured-stderr");
            std::io::stderr().flush().unwrap();
        }
        Ok("tree") => {
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "process_helper", "--nocapture"])
                .env(HELPER_ENV, "descendant")
                .env(MARKER_ENV, std::env::var_os(MARKER_ENV).unwrap())
                .spawn()
                .unwrap();
            std::thread::sleep(Duration::from_secs(30));
        }
        Ok("descendant") => {
            std::thread::sleep(Duration::from_millis(250));
            std::fs::write(std::env::var_os(MARKER_ENV).unwrap(), b"alive").unwrap();
            std::thread::sleep(Duration::from_secs(30));
        }
        _ => {}
    }
}

#[test]
fn captures_stdout_and_stderr() {
    let output = create_helper("output")
        .exec_capture_output_to_memory_blocking(&CaptureOptions::default())
        .unwrap();

    assert!(output.success());
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("captured-stdout"),
        "stdout was {:?}",
        output.stdout
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("captured-stderr"),
        "stderr was {:?}",
        output.stderr
    );
}

#[test]
fn timeout_terminates_job_descendants() {
    let marker = temp_path("windows-descendant");
    let error = create_helper("tree")
        .env(MARKER_ENV, &marker)
        .exec_capture_output_to_memory_blocking(&CaptureOptions {
            timeout: Some(Duration::from_millis(100)),
            ..CaptureOptions::default()
        })
        .unwrap_err();

    assert!(matches!(
        error.downcast_ref::<ProcessError>().unwrap(),
        ProcessError::Timeout { .. }
    ));

    std::thread::sleep(Duration::from_millis(500));
    assert!(!marker.exists());
}
