#![cfg(unix)]

use starbase_console::{Console, EmptyReporter};
use starbase_process::{ChildExit, Command, ProcessError, ProcessRegistry, ShellType, SignalType};
use std::time::Duration;

fn create_command(script: &str) -> Command<EmptyReporter> {
    let mut command = Command::new("bash");
    command.args(["-c", script]);
    command.no_shell();
    command.set_console(Console::new_testing());
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
    async fn drains_output_while_writing_stdin() {
        const SIZE: usize = 128 * 1024;

        // The child fills stdout before reading stdin. Writing all input
        // before draining stdout would block both sides on their pipe buffer.
        let mut command = create_command(&format!("head -c {SIZE} /dev/zero; cat"));
        command.input(["x".repeat(SIZE)]);

        let output = tokio::time::timeout(Duration::from_secs(5), command.exec_capture_output())
            .await
            .expect("command deadlocked")
            .unwrap();

        assert_eq!(output.stdout.len(), SIZE * 2);
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
        assert_eq!(output.stdout.as_ref(), b"one\ntwo\n");
    }

    #[tokio::test]
    async fn preserves_non_utf8_bytes_and_line_endings() {
        let mut command = create_command(r"printf 'one\r\ntwo\xff\n'; printf 'err\r\n\xff' 1>&2");
        command.set_continuous_pipe(true);

        let output = command.exec_capture_output().await.unwrap();

        assert_eq!(output.stdout.as_ref(), b"one\r\ntwo\xff\n");
        assert_eq!(output.stderr.as_ref(), b"err\r\n\xff");
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

mod cancellation {
    use super::*;

    async fn force_kill_unblocks_inherited_stdin(continuous_pipe: bool) {
        let marker = std::env::temp_dir().join(format!(
            "starbase-process-stdin-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&marker);

        // The background process inherits stdin but never reads it. Once the
        // direct shell is killed, the input writer must stop instead of waiting
        // for this process to exit and close its inherited pipe.
        let mut command = create_command(&format!(
            "sleep 30 <&0 & echo \"$$ $!\" > '{}'; wait",
            marker.display()
        ));
        command.set_continuous_pipe(continuous_pipe);
        command.set_error_on_nonzero(false);
        command.input(vec!["x".repeat(1024); 2048]);

        let mut task = tokio::spawn(async move { command.exec_capture_output().await });
        let registry = ProcessRegistry::instance();

        let (child, descendant) = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if let Ok(pids) = std::fs::read_to_string(&marker) {
                    let mut pids = pids
                        .split_whitespace()
                        .filter_map(|pid| pid.parse::<u32>().ok());

                    if let (Some(pid), Some(descendant)) = (pids.next(), pids.next())
                        && let Some(child) = registry.get_running_by_pid(pid).await
                    {
                        break (child, descendant);
                    }
                }

                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("child was not registered");

        child.kill_with_signal(SignalType::Kill).await.unwrap();

        let result = tokio::time::timeout(Duration::from_secs(1), &mut task).await;

        starbase_process::kill(descendant, SignalType::Kill).unwrap();
        std::fs::remove_file(marker).unwrap();

        let output = match result {
            Ok(result) => result.unwrap().unwrap(),
            Err(_) => {
                task.abort();
                let _ = task.await;
                panic!("force-killed command remained blocked writing stdin");
            }
        };

        assert_eq!(output.exit, ChildExit::Killed);
    }

    #[tokio::test]
    async fn aborting_execution_kills_and_unregisters_the_child() {
        let marker = std::env::temp_dir().join(format!(
            "starbase-process-cancel-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&marker);

        // `exec` keeps the recorded pid as the direct child, avoiding a
        // background shell process that would outlive this test.
        let mut command =
            create_command(&format!("echo $$ > '{}'; exec sleep 30", marker.display()));
        let task = tokio::spawn(async move { command.exec_capture_output().await });
        let registry = ProcessRegistry::instance();

        let pid = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if let Ok(pid) = std::fs::read_to_string(&marker)
                    && let Ok(pid) = pid.trim().parse()
                    && registry.get_running_by_pid(pid).await.is_some()
                {
                    break pid;
                }

                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("child was not registered");

        let child = registry.get_running_by_pid(pid).await.unwrap();

        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());

        tokio::time::timeout(Duration::from_secs(1), async {
            while registry.get_running_by_pid(pid).await.is_some() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }

            assert_eq!(child.wait().await.unwrap(), ChildExit::Killed);
        })
        .await
        .expect("cancelled child remained registered");

        std::fs::remove_file(marker).unwrap();
    }

    #[tokio::test]
    async fn force_kill_unblocks_buffered_input_with_inherited_stdin() {
        force_kill_unblocks_inherited_stdin(false).await;
    }

    #[tokio::test]
    async fn force_kill_unblocks_continuous_input_with_inherited_stdin() {
        force_kill_unblocks_inherited_stdin(true).await;
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

    #[tokio::test]
    async fn cached_nonzero_output_respects_error_policy() {
        let mut allowed = create_command("exit 3");
        allowed.set_cache(true).set_error_on_nonzero(false);

        let output = allowed.exec_capture_output().await.unwrap();
        assert_eq!(output.code(), Some(3));

        let mut required = create_command("exit 3");
        required.set_cache(true);

        assert!(matches!(
            required
                .exec_capture_output()
                .await
                .unwrap_err()
                .downcast_ref::<ProcessError>(),
            Some(ProcessError::ExitNonZeroWithOutput { .. })
        ));
    }

    #[tokio::test]
    async fn keeps_capture_modes_in_separate_cache_entries() {
        let mut capture = create_command(r"printf 'one\rtwo'");
        capture.set_cache(true);

        let output = capture.exec_capture_output().await.unwrap();
        assert_eq!(output.stdout.as_ref(), b"one\rtwo");

        let mut stream_capture = create_command(r"printf 'one\rtwo'");
        stream_capture.set_cache(true);

        let output = stream_capture
            .exec_stream_and_capture_output()
            .await
            .unwrap();
        assert_eq!(output.stdout.as_ref(), b"two");
    }
}

mod shells {
    use super::*;

    #[tokio::test]
    async fn runs_scripts_through_a_shell() {
        let mut command: Command<EmptyReporter> = Command::new_script("printf 'one'; printf 'two'");
        command.set_console(Console::new_testing());

        let output = command.exec_capture_output().await.unwrap();

        assert_eq!(output.stdout.as_ref(), b"onetwo");
    }

    #[tokio::test]
    async fn runs_binaries_through_a_shell() {
        let mut command: Command<EmptyReporter> = Command::new("printf");
        command.arg("with space");
        command.set_shell(ShellType::Bash);
        command.set_console(Console::new_testing());

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
        command.set_console(Console::new_testing());
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
