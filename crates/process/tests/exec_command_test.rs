#![cfg(unix)]

use starbase_console::{Console, EmptyReporter};
use starbase_process::{ChildExit, Command, ProcessError, ShellType};
use std::sync::Arc;

fn create_command(script: &str) -> Command<EmptyReporter> {
    let mut command = Command::new("bash");
    command.args(["-c", script]);
    command.no_shell();
    command.set_console(Arc::new(Console::new_testing()));
    command
}

mod exec_capture_output {
    use super::*;

    #[tokio::test]
    async fn captures_stdout_and_stderr() {
        let output = create_command("printf 'out'; printf 'err' 1>&2")
            .exec_capture_output()
            .await
            .unwrap();

        assert!(output.success());
        assert_eq!(output.stdout.as_ref(), b"out");
        assert_eq!(output.stderr.as_ref(), b"err");
    }

    #[tokio::test]
    async fn errors_on_nonzero_exit() {
        let error = create_command("echo 'oops' 1>&2; exit 3")
            .exec_capture_output()
            .await
            .unwrap_err();

        match error.downcast_ref::<ProcessError>().unwrap() {
            ProcessError::ExitNonZeroWithOutput { status, output, .. } => {
                assert_eq!(status, "exit code 3");
                assert!(output.contains("oops"));
            }
            _ => panic!("expected ExitNonZeroWithOutput"),
        };
    }

    #[tokio::test]
    async fn can_allow_nonzero_exit() {
        let mut command = create_command("exit 3");
        command.set_error_on_nonzero(false);

        let output = command.exec_capture_output().await.unwrap();

        assert!(!output.success());
        assert_eq!(output.code(), Some(3));
    }

    #[tokio::test]
    async fn passes_input_to_stdin() {
        let mut command = create_command("cat");
        command.input(["hello", "world"]);

        let output = command.exec_capture_output().await.unwrap();

        assert_eq!(output.stdout.as_ref(), b"hello world");
    }

    #[tokio::test]
    async fn survives_child_exiting_before_consuming_stdin() {
        // The child exits without reading stdin while we write input far
        // larger than any pipe buffer, so the writer hits a broken pipe.
        // That must be benign: the child's exit status is the outcome.
        let mut command = create_command("exit 0");
        command.input(vec!["x".repeat(1024); 2048]);

        let output = command.exec_capture_output().await.unwrap();

        assert!(output.success());
    }

    #[tokio::test]
    async fn reports_killed_children() {
        let mut command = create_command("kill -9 $$");
        command.set_error_on_nonzero(false);

        let output = command.exec_capture_output().await.unwrap();

        assert!(!output.success());
        assert_eq!(output.exit, ChildExit::Killed);
    }
}

mod exec_capture_continuous_output {
    use super::*;

    #[tokio::test]
    async fn pipes_input_and_captures_output() {
        let mut command = create_command("cat");
        command.set_continuous_pipe(true);
        command.input(["one\n", "two\n"]);

        let output = command.exec_capture_output().await.unwrap();

        assert!(output.success());
        assert_eq!(output.stdout.as_ref(), b"one\ntwo");
    }

    #[tokio::test]
    async fn survives_child_exiting_before_consuming_stdin() {
        // The child exits without reading stdin while we stream input far
        // larger than any pipe buffer, so the writer hits a broken pipe.
        // That must be benign: the child's exit status is the outcome
        // (a consumer previously died silently with exit code 141 here,
        // when SIGPIPE was reset to its default disposition).
        let mut command = create_command("exit 0");
        command.set_continuous_pipe(true);
        command.input(vec!["x".repeat(1024); 2048]);

        let output = command.exec_capture_output().await.unwrap();

        assert!(output.success());
    }
}

mod exec_stream_output {
    use super::*;

    #[tokio::test]
    async fn returns_empty_output() {
        let output = create_command("printf 'streamed'")
            .exec_stream_output()
            .await
            .unwrap();

        assert!(output.success());
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }

    #[tokio::test]
    async fn errors_on_nonzero_exit() {
        let error = create_command("exit 1")
            .exec_stream_output()
            .await
            .unwrap_err();

        assert!(matches!(
            error.downcast_ref::<ProcessError>().unwrap(),
            ProcessError::ExitNonZero { .. }
        ));
    }
}

mod child_env {
    use super::*;

    #[tokio::test]
    async fn sets_env_vars() {
        let mut command = create_command(r#"printf "${STARBASE_TEST_SET_VAR:-missing}""#);
        command.env("STARBASE_TEST_SET_VAR", "value");

        let output = command.exec_capture_output().await.unwrap();

        assert_eq!(output.stdout.as_ref(), b"value");
    }

    #[tokio::test]
    async fn unsets_env_vars() {
        let mut command = create_command(r#"printf "${HOME:-unset}""#);
        command.env_remove("HOME");

        let output = command.exec_capture_output().await.unwrap();

        assert_eq!(output.stdout.as_ref(), b"unset");
    }

    #[tokio::test]
    async fn sets_cwd_and_pwd() {
        let dir = std::env::temp_dir().canonicalize().unwrap();

        let mut command = create_command(r#"printf "$PWD""#);
        command.cwd(&dir);

        let output = command.exec_capture_output().await.unwrap();

        assert_eq!(output.stdout, dir.as_os_str().as_encoded_bytes());
    }

    #[tokio::test]
    async fn prepends_lookup_paths() {
        let mut command = create_command(r#"printf "$PATH""#);
        command.prepend_paths(["/starbase-test-fake-path"]);

        let output = command.exec_capture_output().await.unwrap();

        assert!(output.stdout.starts_with(b"/starbase-test-fake-path:"));
    }
}

mod exec_stream_and_capture_output {
    use super::*;

    #[tokio::test]
    async fn keeps_trailing_newlines() {
        let output = create_command(r"printf 'a\nb\n'; printf 'err' 1>&2")
            .exec_stream_and_capture_output()
            .await
            .unwrap();

        assert!(output.success());
        assert_eq!(output.stdout.as_ref(), b"a\nb\n");
        assert_eq!(output.stderr.as_ref(), b"err");
    }

    #[tokio::test]
    async fn captures_stdout_and_stderr() {
        let output = create_command("printf 'out'; printf 'err' 1>&2")
            .exec_stream_and_capture_output()
            .await
            .unwrap();

        assert!(output.success());
        assert_eq!(output.stdout.as_ref(), b"out");
        assert_eq!(output.stderr.as_ref(), b"err");
    }

    #[tokio::test]
    async fn preserves_non_utf8_bytes() {
        let output = create_command(r"printf 'a\xffb'")
            .exec_stream_and_capture_output()
            .await
            .unwrap();

        assert_eq!(output.stdout.as_ref(), b"a\xffb");
    }

    #[tokio::test]
    async fn collapses_carriage_return_redraws() {
        let output = create_command(r"printf '1/3\r2/3\r3/3 done\nnext\n'")
            .exec_stream_and_capture_output()
            .await
            .unwrap();

        assert_eq!(output.stdout.as_ref(), b"3/3 done\nnext\n");
    }

    #[tokio::test]
    async fn keeps_crlf_line_endings() {
        let output = create_command(r"printf 'one\r\ntwo\r\n'")
            .exec_stream_and_capture_output()
            .await
            .unwrap();

        assert_eq!(output.stdout.as_ref(), b"one\r\ntwo\r\n");
    }
}

mod caching {
    use super::*;

    // The pid changes on every spawn, so identical output means the
    // second run was served from the cache
    fn create_pid_command(marker: &str) -> Command<EmptyReporter> {
        let mut command = create_command(&format!("printf '{marker}'; printf $$"));
        command.set_cache(true);
        command
    }

    #[tokio::test]
    async fn reuses_output_for_identical_commands() {
        let first = create_pid_command("a").exec_capture_output().await.unwrap();
        let second = create_pid_command("a").exec_capture_output().await.unwrap();

        assert_eq!(first.stdout, second.stdout);
    }

    #[tokio::test]
    async fn does_not_reuse_output_for_different_commands() {
        let first = create_pid_command("b").exec_capture_output().await.unwrap();
        let second = create_pid_command("c").exec_capture_output().await.unwrap();

        assert_ne!(first.stdout, second.stdout);
    }

    #[tokio::test]
    async fn does_not_cache_when_disabled() {
        let mut first = create_pid_command("d");
        first.set_cache(false);

        let mut second = create_pid_command("d");
        second.set_cache(false);

        assert_ne!(
            first.exec_capture_output().await.unwrap().stdout,
            second.exec_capture_output().await.unwrap().stdout
        );
    }

    #[tokio::test]
    async fn caches_streamed_output_too() {
        let first = create_pid_command("e")
            .exec_stream_and_capture_output()
            .await
            .unwrap();
        let second = create_pid_command("e")
            .exec_stream_and_capture_output()
            .await
            .unwrap();

        assert_eq!(first.stdout, second.stdout);
    }
}

mod shells {
    use super::*;

    #[tokio::test]
    async fn runs_scripts_through_a_shell() {
        let mut command: Command<EmptyReporter> = Command::new_script("printf 'one'; printf 'two'");
        command.set_console(Arc::new(Console::new_testing()));

        let output = command.exec_capture_output().await.unwrap();

        assert_eq!(output.stdout.as_ref(), b"onetwo");
    }

    #[tokio::test]
    async fn runs_binaries_through_a_shell() {
        let mut command: Command<EmptyReporter> = Command::new("printf");
        command.arg("with space");
        command.set_shell(ShellType::Bash);
        command.set_console(Arc::new(Console::new_testing()));

        let output = command.exec_capture_output().await.unwrap();

        // The arg is quoted for the shell, so it arrives as one arg
        assert_eq!(output.stdout.as_ref(), b"with space");
    }
}

mod spawn_failures {
    use super::*;

    fn create_missing_command() -> Command<EmptyReporter> {
        let mut command: Command<EmptyReporter> = Command::new("starbase-does-not-exist");
        command.no_shell();
        command.set_console(Arc::new(Console::new_testing()));
        command
    }

    #[tokio::test]
    async fn capture_reports_a_capture_error() {
        let error = create_missing_command()
            .exec_capture_output()
            .await
            .unwrap_err();

        assert!(matches!(
            error.downcast_ref::<ProcessError>().unwrap(),
            ProcessError::Capture { .. }
        ));
    }

    #[tokio::test]
    async fn stream_reports_a_stream_error() {
        let error = create_missing_command()
            .exec_stream_output()
            .await
            .unwrap_err();

        assert!(matches!(
            error.downcast_ref::<ProcessError>().unwrap(),
            ProcessError::Stream { .. }
        ));
    }

    #[tokio::test]
    async fn stream_and_capture_reports_a_stream_capture_error() {
        let error = create_missing_command()
            .exec_stream_and_capture_output()
            .await
            .unwrap_err();

        assert!(matches!(
            error.downcast_ref::<ProcessError>().unwrap(),
            ProcessError::StreamCapture { .. }
        ));
    }

    #[tokio::test]
    async fn errors_name_the_binary() {
        let error = create_missing_command()
            .exec_capture_output()
            .await
            .unwrap_err();

        assert!(error.to_string().contains("starbase-does-not-exist"));
    }
}
