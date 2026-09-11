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
fn rejects_unsupported_options_before_spawning() {
    for (option, configure) in [
        ("buffered input", 0_u8),
        ("output caching", 1),
        ("continuous pipes", 2),
    ] {
        let marker = temp_path(option);
        let mut command = create_command(&format!("touch '{}'", marker.display()));

        match configure {
            0 => {
                command.input(["hello"]);
            }
            1 => {
                command.set_cache(true);
            }
            _ => {
                command.set_continuous_pipe(true);
            }
        }

        let error = command
            .exec_capture_output_to_memory_blocking(&CaptureOptions::default())
            .unwrap_err();

        assert!(matches!(
            error.downcast_ref::<ProcessError>().unwrap(),
            ProcessError::UnsupportedCaptureOption {
                option: actual,
                ..
            } if *actual == option
        ));
        assert!(!marker.exists());
    }
}

#[test]
fn enforces_the_combined_output_limit() {
    let error = create_command("printf '1234'; printf '5678' 1>&2; sleep 5")
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
fn output_limit_after_parent_exit_terminates_descendants() {
    let marker = temp_path("post-exit-output-limit");
    let script = format!(
        "(sleep 0.05; printf '12345678'; sleep 0.2; touch '{}') & exit 0",
        marker.display()
    );
    let error = create_command(&script)
        .exec_capture_output_to_memory_blocking(&CaptureOptions {
            output_limit: Some(7),
            ..CaptureOptions::default()
        })
        .unwrap_err();

    assert!(matches!(
        error.downcast_ref::<ProcessError>().unwrap(),
        ProcessError::OutputLimitExceeded { .. }
    ));

    std::thread::sleep(Duration::from_millis(350));
    assert!(!marker.exists());
}

#[test]
fn normal_completion_does_not_kill_a_descendant_holding_a_pipe() {
    let marker = temp_path("normal-descendant");
    let script = format!("(sleep 0.2; touch '{}') & printf 'ready'", marker.display());
    let error = create_command(&script)
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

    std::thread::sleep(Duration::from_millis(350));
    assert!(marker.exists());
    std::fs::remove_file(marker).unwrap();
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

#[cfg(target_os = "linux")]
#[test]
fn failure_cleanup_stays_bounded_when_normal_drain_is_unbounded() {
    let pid_file = temp_path("detached-pid");
    let script = format!(
        "setsid sh -c 'echo $$ > \"{0}\"; sleep 30' & while [ ! -s \"{0}\" ]; do sleep 0.001; done; sleep 30",
        pid_file.display(),
    );
    let start = std::time::Instant::now();
    let error = create_command(&script)
        .exec_capture_output_to_memory_blocking(&CaptureOptions {
            timeout: Some(Duration::from_millis(50)),
            output_drain_timeout: None,
            ..CaptureOptions::default()
        })
        .unwrap_err();

    assert!(matches!(
        error.downcast_ref::<ProcessError>().unwrap(),
        ProcessError::Timeout { .. }
    ));
    assert!(start.elapsed() < Duration::from_secs(2));

    let pid = std::fs::read_to_string(&pid_file)
        .unwrap()
        .trim()
        .parse::<i32>()
        .unwrap();
    unsafe { libc::kill(pid, libc::SIGKILL) };
    std::fs::remove_file(pid_file).unwrap();
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
