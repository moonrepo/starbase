// These tests run on every platform. Rather than a shell, the child is
// this test binary itself, run again with `process_helper` selected, which
// behaves according to `HELPER_ENV`. (This approach is from
// moonrepo/starbase#213.)

use starbase_console::{Console, EmptyReporter};
use starbase_process::{ChildExit, Command, ProcessError, ProcessSet, SignalType};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

const HELPER_ENV: &str = "STARBASE_PROCESS_TEST_HELPER";
const MARKER_ENV: &str = "STARBASE_PROCESS_TEST_MARKER";

fn create_helper(mode: &str) -> Command<EmptyReporter> {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args(["--exact", "process_helper", "--nocapture"]);
    command.env(HELPER_ENV, mode);
    command.no_shell();
    command.set_console(Console::new_testing());
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

fn assert_timed_out(error: miette::Report) {
    assert!(
        matches!(
            error.downcast_ref::<ProcessError>(),
            Some(ProcessError::Timeout { .. })
        ),
        "expected a timeout, got {error:?}"
    );
}

#[test]
fn process_helper() {
    match std::env::var(HELPER_ENV).as_deref() {
        Ok("output") => {
            print!("captured");
            std::io::stdout().flush().unwrap();
        }
        Ok("sleep") => {
            thread::sleep(Duration::from_secs(30));
        }
        // Start a descendant, and wait for it
        Ok("tree") => {
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "process_helper", "--nocapture"])
                .env(HELPER_ENV, "descendant")
                .env(MARKER_ENV, std::env::var_os(MARKER_ENV).unwrap())
                .spawn()
                .unwrap()
                .wait()
                .unwrap();
        }
        // Grow the marker file for as long as this process runs
        Ok("descendant") => {
            let mut marker = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(std::env::var_os(MARKER_ENV).unwrap())
                .unwrap();

            for _ in 0..600 {
                marker.write_all(b".").unwrap();
                thread::sleep(Duration::from_millis(50));
            }
        }
        _ => {}
    }
}

#[tokio::test]
async fn interrupting_a_child_returns_on_every_platform() {
    let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "process_helper", "--nocapture"])
        .env(HELPER_ENV, "sleep");

    let child = ProcessSet::root().nested().spawn(command).unwrap();
    let exit = tokio::time::timeout(
        Duration::from_secs(10),
        child.kill_with_signal(SignalType::Interrupt),
    )
    .await
    .expect("interrupt did not end the child")
    .unwrap();

    // Windows can't send `CTRL-C` to a single process, so it force kills
    assert_eq!(
        exit,
        if cfg!(windows) {
            ChildExit::Killed
        } else {
            ChildExit::Interrupted
        }
    );
}

#[tokio::test]
async fn commands_that_finish_in_time_succeed() {
    let mut command = create_helper("output");
    command.set_timeout(Duration::from_secs(30));

    let output = command.exec_capture_output().await.unwrap();

    assert!(String::from_utf8_lossy(&output.stdout).contains("captured"));
}

#[tokio::test]
async fn a_timeout_kills_the_child() {
    let mut command = create_helper("sleep");
    command.set_timeout(Duration::from_millis(500));

    let started = Instant::now();
    let error = command.exec_capture_output().await.unwrap_err();

    assert_timed_out(error);
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn a_timeout_kills_the_whole_tree() {
    let marker = temp_path("tree");
    let mut command = create_helper("tree");
    command.env(MARKER_ENV, &marker);
    command.set_timeout(Duration::from_secs(3));

    assert_timed_out(command.exec_capture_output_blocking().unwrap_err());

    // The descendant ran, and stopped growing the marker once killed
    thread::sleep(Duration::from_millis(200));

    let size = fs::metadata(&marker).map(|meta| meta.len()).unwrap_or(0);

    assert!(size > 0, "descendant never started");

    thread::sleep(Duration::from_millis(500));

    assert_eq!(
        fs::metadata(&marker).unwrap().len(),
        size,
        "descendant was not killed"
    );

    let _ = fs::remove_file(marker);
}
