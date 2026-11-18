#![cfg(unix)]

use starbase_console::{Console, EmptyReporter};
use starbase_process::{ChildExit, Command, ProcessError, ProcessSet, SignalType};
use std::fs;
use std::thread;
use std::time::{Duration, Instant};
use tokio::runtime::Builder;

fn create_command(script: &str) -> Command<EmptyReporter> {
    let mut command = Command::new("bash");
    command.args(["-c", script]);
    command.no_shell();
    command.set_console(Console::new_testing());
    command
}

#[test]
fn captures_stdout_and_stderr() {
    let output = create_command("printf 'out'; printf 'err' 1>&2")
        .exec_capture_output_blocking()
        .unwrap();

    assert!(output.success());
    assert_eq!(output.stdout.as_ref(), b"out");
    assert_eq!(output.stderr.as_ref(), b"err");
}

#[test]
fn errors_on_nonzero_exit() {
    let error = create_command("echo 'oops' 1>&2; exit 3")
        .exec_capture_output_blocking()
        .unwrap_err();

    match error.downcast_ref::<ProcessError>().unwrap() {
        ProcessError::ExitNonZeroWithOutput { status, output, .. } => {
            assert_eq!(status, "exit code 3");
            assert!(output.contains("oops"));
        }
        _ => panic!("expected ExitNonZeroWithOutput"),
    };
}

#[test]
fn can_allow_nonzero_exit() {
    let mut command = create_command("exit 3");
    command.set_error_on_nonzero(false);

    let output = command.exec_capture_output_blocking().unwrap();

    assert_eq!(output.code(), Some(3));
}

#[test]
fn passes_input_to_stdin() {
    let mut command = create_command("cat");
    command.input(["hello"]);

    let output = command.exec_capture_output_blocking().unwrap();

    assert_eq!(output.stdout.as_ref(), b"hello");
}

#[test]
fn streams_output() {
    let mut command = create_command("printf 'out'; exit 3");
    command.set_error_on_nonzero(false);

    let output = command.exec_stream_output_blocking().unwrap();

    assert_eq!(output.code(), Some(3));
    assert!(output.stdout.is_empty());
}

#[test]
fn streams_and_captures_output() {
    let output = create_command("printf 'out'; printf 'err' 1>&2")
        .exec_stream_and_capture_output_blocking()
        .unwrap();

    assert_eq!(output.stdout.as_ref(), b"out");
    assert_eq!(output.stderr.as_ref(), b"err");
}

#[tokio::test]
async fn runs_inside_a_current_thread_runtime() {
    let output = create_command("printf 'out'")
        .exec_capture_output_blocking()
        .unwrap();

    assert_eq!(output.stdout.as_ref(), b"out");
}

// Like a sync WASM host function, which runs on a blocking thread
#[test]
fn runs_on_a_blocking_thread_of_a_runtime() {
    let runtime = Builder::new_multi_thread().enable_all().build().unwrap();

    let output = runtime
        .block_on(
            runtime
                .spawn_blocking(|| create_command("printf 'out'").exec_capture_output_blocking()),
        )
        .unwrap()
        .unwrap();

    assert_eq!(output.stdout.as_ref(), b"out");
}

#[test]
fn a_set_shutdown_stops_a_blocking_command() {
    let marker =
        std::env::temp_dir().join(format!("starbase-exec-blocking-{}", std::process::id()));
    let _ = fs::remove_file(&marker);

    let set = ProcessSet::root().nested_with_grace(Duration::from_secs(1));
    let mut command = create_command(&format!("echo $$ > '{}'; exec sleep 30", marker.display()));
    command.set_process_set(set.clone());
    command.set_error_on_nonzero(false);

    let exec = thread::spawn(move || command.exec_capture_output_blocking());

    let started = Instant::now();
    let pid = loop {
        if let Some(pid) = fs::read_to_string(&marker)
            .ok()
            .and_then(|pid| pid.trim().parse::<u32>().ok())
        {
            break pid;
        }

        assert!(
            started.elapsed() < Duration::from_secs(3),
            "command did not start"
        );
        thread::sleep(Duration::from_millis(5));
    };

    assert!(set.get(pid).is_some());

    Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(set.shutdown(SignalType::Terminate));

    let output = exec.join().unwrap().unwrap();

    assert_eq!(
        output.exit,
        ChildExit::Terminated(SignalType::Terminate.get_code())
    );
    assert!(set.get(pid).is_none());

    fs::remove_file(marker).unwrap();
}
