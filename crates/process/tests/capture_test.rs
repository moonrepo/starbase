#![cfg(unix)]

use starbase_process::{CaptureError, CaptureOptions, capture_output, capture_output_to_files};
use std::fs::File;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

fn create_command(script: &str) -> Command {
    let mut command = Command::new("sh");
    command.args(["-c", script]);
    command
}

fn create_output_files() -> (std::path::PathBuf, File, std::path::PathBuf, File) {
    static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let prefix = format!("starbase-process-{}-{id}", std::process::id());
    let stdout_path = std::env::temp_dir().join(format!("{prefix}-stdout"));
    let stderr_path = std::env::temp_dir().join(format!("{prefix}-stderr"));
    let stdout = File::create(&stdout_path).unwrap();
    let stderr = File::create(&stderr_path).unwrap();

    (stdout_path, stdout, stderr_path, stderr)
}

#[test]
fn captures_stdout_and_stderr() {
    let output = capture_output(
        &mut create_command("printf 'out'; printf 'err' 1>&2"),
        None,
        &CaptureOptions::default(),
    )
    .unwrap();

    assert!(output.status.success());
    assert_eq!(output.stdout, b"out");
    assert_eq!(output.stderr, b"err");
}

#[test]
fn passes_input_to_stdin() {
    let output = capture_output(
        &mut create_command("cat"),
        Some(b"hello world".to_vec()),
        &CaptureOptions::default(),
    )
    .unwrap();

    assert_eq!(output.stdout, b"hello world");
}

#[test]
fn enforces_timeout() {
    let error = capture_output(
        &mut create_command("sleep 30"),
        None,
        &CaptureOptions {
            timeout: Some(Duration::from_millis(50)),
            ..CaptureOptions::default()
        },
    )
    .unwrap_err();

    assert!(matches!(error, CaptureError::Timeout { .. }));
}

#[test]
fn enforces_combined_output_limit() {
    let error = capture_output(
        &mut create_command("printf '12345'; printf '67890' 1>&2"),
        None,
        &CaptureOptions {
            output_limit: Some(8),
            ..CaptureOptions::default()
        },
    )
    .unwrap_err();

    assert!(matches!(
        error,
        CaptureError::OutputLimitExceeded { limit: 8 }
    ));
}

#[test]
fn never_returns_truncated_output_at_the_limit() {
    for _ in 0..100 {
        let result = capture_output(
            &mut create_command("printf '123456789'"),
            None,
            &CaptureOptions {
                output_limit: Some(8),
                ..CaptureOptions::default()
            },
        );

        assert!(matches!(
            result.unwrap_err(),
            CaptureError::OutputLimitExceeded { limit: 8 }
        ));
    }
}

#[test]
fn timeout_terminates_descendants_holding_output_pipes() {
    let error = capture_output(
        &mut create_command("sleep 30 & printf 'ready'"),
        None,
        &CaptureOptions {
            timeout: Some(Duration::from_millis(50)),
            ..CaptureOptions::default()
        },
    )
    .unwrap_err();

    assert!(matches!(error, CaptureError::Timeout { .. }));
}

#[test]
fn completion_terminates_detached_descendants() {
    let output = capture_output(
        &mut create_command("sleep 30 >/dev/null 2>&1 & printf $!"),
        None,
        &CaptureOptions::default(),
    )
    .unwrap();
    let pid = std::str::from_utf8(&output.stdout)
        .unwrap()
        .parse::<i32>()
        .unwrap();

    for _ in 0..20 {
        if unsafe { libc::kill(pid, 0) } == -1
            && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
        {
            return;
        }

        std::thread::sleep(Duration::from_millis(10));
    }

    panic!("descendant process {pid} survived capture completion");
}

#[test]
fn preserves_non_utf8_bytes() {
    let output = capture_output(
        &mut create_command(r"printf 'a\377b'"),
        None,
        &CaptureOptions::default(),
    )
    .unwrap();

    assert_eq!(output.stdout, b"a\xffb");
}

#[test]
fn captures_output_to_files() {
    let (stdout_path, stdout, stderr_path, stderr) = create_output_files();
    let output = capture_output_to_files(
        &mut create_command("printf 'out'; printf 'err' 1>&2"),
        None,
        &CaptureOptions::default(),
        stdout,
        stderr,
    )
    .unwrap();

    assert_eq!(output.stdout_len, 3);
    assert_eq!(output.stderr_len, 3);
    assert_eq!(std::fs::read(&stdout_path).unwrap(), b"out");
    assert_eq!(std::fs::read(&stderr_path).unwrap(), b"err");

    std::fs::remove_file(stdout_path).unwrap();
    std::fs::remove_file(stderr_path).unwrap();
}

#[test]
fn enforces_output_limit_when_capturing_to_files() {
    let (stdout_path, stdout, stderr_path, stderr) = create_output_files();
    let error = capture_output_to_files(
        &mut create_command("printf '123456789'"),
        None,
        &CaptureOptions {
            output_limit: Some(8),
            ..CaptureOptions::default()
        },
        stdout,
        stderr,
    )
    .unwrap_err();

    assert!(matches!(
        error,
        CaptureError::OutputLimitExceeded { limit: 8 }
    ));

    std::fs::remove_file(stdout_path).unwrap();
    std::fs::remove_file(stderr_path).unwrap();
}
