#![cfg(unix)]

use starbase_console::{Console, EmptyReporter};
use starbase_process::{CaptureOptions, Command, ProcessError};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

fn create_command(script: &str) -> Command<EmptyReporter> {
    let mut command = Command::new("bash");
    command.args(["-c", script]);
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
fn captures_stdout_and_stderr() {
    let output = create_command("printf 'out'; printf 'err' 1>&2")
        .exec_capture_output_to_memory_blocking(&CaptureOptions::default())
        .unwrap();

    assert!(output.success());
    assert_eq!(output.stdout.as_ref(), b"out");
    assert_eq!(output.stderr.as_ref(), b"err");
}

#[test]
fn passes_input_to_stdin() {
    let mut command = create_command("cat");
    command.input(["hello", "world"]);

    let output = command
        .exec_capture_output_to_memory_blocking(&CaptureOptions::default())
        .unwrap();

    assert_eq!(output.stdout.as_ref(), b"hello world");
}

#[test]
fn survives_child_exiting_before_consuming_stdin() {
    let mut command = create_command("exit 0");
    command.input(vec!["x".repeat(1024); 2048]);

    let output = command
        .exec_capture_output_to_memory_blocking(&CaptureOptions::default())
        .unwrap();

    assert!(output.success());
}

#[test]
fn enforces_the_combined_output_limit() {
    let error = create_command("printf '1234'; printf '5678' 1>&2")
        .exec_capture_output_to_memory_blocking(&CaptureOptions {
            output_limit: Some(7),
            ..CaptureOptions::default()
        })
        .unwrap_err();

    let process_error = error.downcast_ref::<ProcessError>().unwrap();
    match process_error {
        ProcessError::OutputLimitExceeded {
            limit: 7,
            output: Some(output),
            ..
        } => assert!(output.stdout.len() + output.stderr.len() <= 7),
        _ => panic!("unexpected error: {process_error:?}"),
    }
}

#[test]
fn completion_terminates_descendants_holding_output_pipes() {
    let start = std::time::Instant::now();
    let output = create_command("sleep 30 & printf 'ready'")
        .exec_capture_output_to_memory_blocking(&CaptureOptions::default())
        .unwrap();

    assert!(output.success());
    assert_eq!(output.stdout.as_ref(), b"ready");
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[cfg(target_os = "linux")]
#[test]
fn reports_output_drain_timeout_for_detached_descendants() {
    let error = create_command("setsid sh -c 'sleep 1' & printf 'ready'")
        .exec_capture_output_to_memory_blocking(&CaptureOptions {
            output_drain_timeout: Some(Duration::from_millis(50)),
            ..CaptureOptions::default()
        })
        .unwrap_err();

    match error.downcast_ref::<ProcessError>().unwrap() {
        ProcessError::OutputDrainTimeout {
            output: Some(output),
            ..
        } => assert_eq!(output.stdout.as_ref(), b"ready"),
        error => panic!("unexpected error: {error:?}"),
    }
}

#[test]
fn timeout_terminates_descendants() {
    let marker = temp_path("descendant");
    let script = format!(
        "(sleep 0.2; touch '{}') & sleep 5",
        marker.to_string_lossy()
    );
    let error = create_command(&script)
        .exec_capture_output_to_memory_blocking(&CaptureOptions {
            timeout: Some(Duration::from_millis(50)),
            ..CaptureOptions::default()
        })
        .unwrap_err();

    assert!(matches!(
        error.downcast_ref::<ProcessError>().unwrap(),
        ProcessError::Timeout { .. }
    ));

    std::thread::sleep(Duration::from_millis(350));
    assert!(!marker.exists());
}

#[test]
fn timeout_preserves_partial_output() {
    let error = create_command("printf 'ready'; sleep 5")
        .exec_capture_output_to_memory_blocking(&CaptureOptions {
            timeout: Some(Duration::from_millis(50)),
            ..CaptureOptions::default()
        })
        .unwrap_err();

    match error.downcast_ref::<ProcessError>().unwrap() {
        ProcessError::Timeout {
            output: Some(output),
            ..
        } => assert_eq!(output.stdout.as_ref(), b"ready"),
        error => panic!("unexpected error: {error:?}"),
    }
}
