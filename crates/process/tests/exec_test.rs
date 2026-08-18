use starbase_console::EmptyReporter;
use starbase_process::ShellType;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

type Command = starbase_process::Command<EmptyReporter>;

fn get_program(command: &std::process::Command) -> OsString {
    command.get_program().to_os_string()
}

fn get_args(command: &std::process::Command) -> Vec<OsString> {
    command.get_args().map(|arg| arg.to_os_string()).collect()
}

fn get_env(command: &std::process::Command, key: &str) -> Option<Option<OsString>> {
    command
        .get_envs()
        .find(|(k, _)| *k == OsStr::new(key))
        .map(|(_, value)| value.map(|value| value.to_os_string()))
}

mod create_sync_command {
    use super::*;

    #[test]
    fn passes_args_separately_without_a_shell() {
        let mut command = Command::new("git");
        command.no_shell().args(["commit", "-m", "with space"]);

        let sync = command.create_sync_command().unwrap();

        assert_eq!(get_program(&sync), OsString::from("git"));
        assert_eq!(get_args(&sync), ["commit", "-m", "with space"]);
    }

    #[test]
    fn joins_args_into_one_string_within_a_shell() {
        let mut command = Command::new("git");
        command.set_shell(ShellType::Bash).args(["commit", "-m"]);

        let sync = command.create_sync_command().unwrap();

        assert_eq!(get_program(&sync), OsString::from("bash"));
        assert_eq!(get_args(&sync), ["-c", "git commit -m"]);
    }

    #[test]
    fn quotes_args_that_need_it_within_a_shell() {
        let mut command = Command::new("git");
        command
            .set_shell(ShellType::Bash)
            .args(["commit", "-m", "with space"]);

        let sync = command.create_sync_command().unwrap();

        // Bash quotes with ANSI-C syntax
        assert_eq!(get_args(&sync), ["-c", "git commit -m $'with space'"]);
    }

    #[test]
    fn wraps_scripts_in_a_shell_even_when_disabled() {
        let mut command = Command::new_script("git status && git log");
        command.no_shell();

        let sync = command.create_sync_command().unwrap();

        // Scripts require a shell, so the default one is used
        assert_eq!(
            get_args(&sync),
            if cfg!(windows) {
                vec![
                    "-NoLogo",
                    "-NoProfile",
                    "-EncodedCommand",
                    "ZwBpAHQAIABzAHQAYQB0AHUAcwAgACYAJgAgAGcAaQB0ACAAbABvAGcA",
                ]
            } else {
                vec!["-c", "git status && git log"]
            }
        );
    }

    #[test]
    fn does_not_quote_the_script() {
        let mut command = Command::new_script("echo 'quoted value'");
        command.set_shell(ShellType::Bash);

        let sync = command.create_sync_command().unwrap();

        assert_eq!(get_args(&sync), ["-c", "echo 'quoted value'"]);
    }

    #[test]
    fn sets_and_removes_env_vars() {
        let mut command = Command::new("git");
        command.no_shell();
        command.env("STARBASE_SET", "value");
        command.env_remove("STARBASE_REMOVED");

        let sync = command.create_sync_command().unwrap();

        assert_eq!(
            get_env(&sync, "STARBASE_SET"),
            Some(Some(OsString::from("value")))
        );
        // A `None` value means the var is removed from the child
        assert_eq!(get_env(&sync, "STARBASE_REMOVED"), Some(None));
    }

    #[test]
    fn only_sets_missing_env_vars() {
        use starbase_process::Env;

        // Borrow a var from the ambient environment rather than setting
        // one, as mutating the environment races with parallel tests
        let (existing_key, _) = std::env::vars_os().next().expect("no environment vars");
        let existing_key = existing_key.to_string_lossy().to_string();

        let mut command = Command::new("git");
        command.no_shell();
        command.env_with_behavior(&existing_key, Env::SetIfMissing(OsString::from("fallback")));
        command.env_with_behavior(
            "STARBASE_NEVER_SET",
            Env::SetIfMissing(OsString::from("fallback")),
        );

        let sync = command.create_sync_command().unwrap();

        // Inherited from the system, so it's not set explicitly
        assert_eq!(get_env(&sync, &existing_key), None);
        assert_eq!(
            get_env(&sync, "STARBASE_NEVER_SET"),
            Some(Some(OsString::from("fallback")))
        );
    }

    #[test]
    fn sets_the_working_dir_and_pwd() {
        let dir = std::env::temp_dir();

        let mut command = Command::new("git");
        command.no_shell().cwd(&dir);

        let sync = command.create_sync_command().unwrap();

        assert_eq!(sync.get_current_dir(), Some(dir.as_path()));
        assert_eq!(get_env(&sync, "PWD"), Some(Some(dir.into_os_string())));
    }

    #[test]
    fn prepends_lookup_paths_to_the_system_path() {
        let mut command = Command::new("git");
        command.no_shell().prepend_paths(["/a", "/b"]);

        let sync = command.create_sync_command().unwrap();
        let path = get_env(&sync, "PATH").unwrap().unwrap();
        let paths = std::env::split_paths(&path).collect::<Vec<_>>();

        assert_eq!(paths[0], PathBuf::from("/a"));
        assert_eq!(paths[1], PathBuf::from("/b"));
        // The system paths follow
        assert!(paths.len() > 2);
    }

    #[test]
    fn appended_paths_come_last() {
        let mut command = Command::new("git");
        command
            .no_shell()
            .prepend_paths(["/a"])
            .append_paths(["/z"]);

        let sync = command.create_sync_command().unwrap();
        let path = get_env(&sync, "PATH").unwrap().unwrap();
        let paths = std::env::split_paths(&path).collect::<Vec<_>>();

        assert_eq!(paths[0], PathBuf::from("/a"));
        assert_eq!(paths[1], PathBuf::from("/z"));
    }

    #[test]
    fn an_explicit_path_wins_over_lookup_paths() {
        let mut command = Command::new("git");
        command
            .no_shell()
            .prepend_paths(["/a"])
            .env("PATH", "/only");

        let sync = command.create_sync_command().unwrap();

        assert_eq!(get_env(&sync, "PATH"), Some(Some(OsString::from("/only"))));
    }

    #[test]
    fn does_not_set_a_path_without_lookup_paths() {
        let mut command = Command::new("git");
        command.no_shell();

        let sync = command.create_sync_command().unwrap();

        assert_eq!(get_env(&sync, "PATH"), None);
    }

    #[test]
    fn removes_bash_env_for_bash_shells() {
        // Non-interactive bash sources `BASH_ENV` after our environment is
        // applied, which would let it overwrite the `PATH` we just set
        let mut command = Command::new("git");
        command.set_shell(ShellType::Bash);

        let sync = command.create_sync_command().unwrap();

        assert_eq!(get_env(&sync, "BASH_ENV"), Some(None));
    }

    #[test]
    fn keeps_an_explicit_bash_env() {
        let mut command = Command::new("git");
        command.set_shell(ShellType::Bash);
        command.env("BASH_ENV", "/setup.sh");

        let sync = command.create_sync_command().unwrap();

        assert_eq!(
            get_env(&sync, "BASH_ENV"),
            Some(Some(OsString::from("/setup.sh")))
        );
    }

    #[test]
    fn does_not_remove_bash_env_for_other_shells() {
        let mut command = Command::new("git");
        command.set_shell(ShellType::Sh);

        let sync = command.create_sync_command().unwrap();

        assert_eq!(get_env(&sync, "BASH_ENV"), None);
    }
}

mod create_async_command {
    use super::*;

    #[test]
    fn mirrors_the_sync_command() {
        let mut command = Command::new("git");
        command.no_shell().arg("status").env("KEY", "value");

        let async_command = command.create_async_command().unwrap();
        let sync = async_command.as_std();

        assert_eq!(get_program(sync), OsString::from("git"));
        assert_eq!(get_args(sync), ["status"]);
        assert_eq!(get_env(sync, "KEY"), Some(Some(OsString::from("value"))));
    }
}
