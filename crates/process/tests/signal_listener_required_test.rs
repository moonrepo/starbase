// Nothing in this binary starts the signal listener, so these tests see
// it as a process that never opted in to signals would.

use starbase_console::{Console, EmptyReporter};
use starbase_process::{Command, ProcessSet};

fn create_command() -> Command<EmptyReporter> {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args(["--exact", "missing_test", "--nocapture"]);
    command.no_shell();
    command.set_console(Console::new_testing());
    command
}

#[tokio::test]
async fn commands_run_without_signals_by_default() {
    // Creating the root, as every exec does, doesn't start the listener
    let output = create_command().exec_capture_output().await.unwrap();

    assert!(output.success());
    assert!(output.pid > 0);
}

#[test]
#[should_panic(expected = "requires the signal listener")]
fn containing_a_child_panics_without_signals() {
    let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
    command.args(["--exact", "missing_test"]);

    let _ = ProcessSet::root().nested().spawn_contained(command);
}

#[tokio::test]
#[should_panic(expected = "requires the signal listener")]
async fn commands_that_contain_their_tree_panic_without_signals() {
    let mut command = create_command();
    command.set_contain_tree(true);

    let _ = command.exec_capture_output().await;
}
