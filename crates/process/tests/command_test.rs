use starbase_console::EmptyReporter;
use starbase_process::{Arg, Env, Executable, ShellType};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

// The command is generic over its console reporter, which the tests
// never exercise, so alias away the type parameter
type Command = starbase_process::Command<EmptyReporter>;

mod arg {
    use super::*;

    #[test]
    fn converts_from_many_types() {
        assert_eq!(Arg::from("str").value, OsString::from("str"));
        assert_eq!(
            Arg::from(String::from("string")).value,
            OsString::from("string")
        );
        assert_eq!(
            Arg::from(OsStr::new("os_str")).value,
            OsString::from("os_str")
        );
        assert_eq!(
            Arg::from(PathBuf::from("path")).value,
            OsString::from("path")
        );
    }

    #[test]
    fn converts_from_borrowed_types() {
        assert_eq!(
            Arg::from(&String::from("string")).value,
            OsString::from("string")
        );
        assert_eq!(
            Arg::from(&OsString::from("os_string")).value,
            OsString::from("os_string")
        );
        assert_eq!(Arg::from(Path::new("path")).value, OsString::from("path"));
        assert_eq!(
            Arg::from(&PathBuf::from("path_buf")).value,
            OsString::from("path_buf")
        );
    }

    #[test]
    fn as_ref_returns_the_quoted_value() {
        let arg = Arg {
            quoted_value: Some(OsString::from("'value'")),
            value: OsString::from("value"),
        };

        assert_eq!(AsRef::<OsStr>::as_ref(&arg), OsStr::new("'value'"));
    }

    #[test]
    fn prefers_quoted_value() {
        let arg = Arg {
            quoted_value: Some(OsString::from("'value'")),
            value: OsString::from("value"),
        };

        assert_eq!(arg.as_os_str(), OsStr::new("'value'"));

        let arg = Arg::from("value");

        assert_eq!(arg.as_os_str(), OsStr::new("value"));
    }
}

mod env {
    use super::*;

    #[test]
    fn returns_value_per_variant() {
        assert_eq!(
            Env::Set(OsString::from("a")).get_value(),
            Some(&OsString::from("a"))
        );
        assert_eq!(
            Env::SetIfMissing(OsString::from("b")).get_value(),
            Some(&OsString::from("b"))
        );
        assert_eq!(Env::Unset.get_value(), None);
    }

    #[test]
    fn returns_os_str_per_variant() {
        assert_eq!(
            Env::Set(OsString::from("a")).as_os_str(),
            Some(OsStr::new("a"))
        );
        assert_eq!(
            Env::SetIfMissing(OsString::from("b")).as_os_str(),
            Some(OsStr::new("b"))
        );
        assert_eq!(Env::Unset.as_os_str(), None);
    }
}

mod executable {
    use super::*;

    #[test]
    fn returns_the_inner_value() {
        assert_eq!(
            Executable::Binary(Arg::from("git")).as_os_str(),
            OsStr::new("git")
        );
        assert_eq!(
            Executable::Script(OsString::from("git status")).as_os_str(),
            OsStr::new("git status")
        );
    }

    #[test]
    fn only_scripts_require_a_shell() {
        assert!(!Executable::Binary(Arg::from("git")).requires_shell());
        assert!(Executable::Script(OsString::from("git status")).requires_shell());
    }

    #[test]
    fn binaries_ignore_the_quoted_value() {
        let exe = Executable::Binary(Arg {
            quoted_value: Some(OsString::from("'my git'")),
            value: OsString::from("my git"),
        });

        assert_eq!(exe.as_os_str(), OsStr::new("my git"));
    }
}

mod args {
    use super::*;

    #[test]
    fn adds_and_lists_args() {
        let mut command = Command::new("git");
        command.arg("status").args(["--short", "--branch"]);

        assert_eq!(command.get_args_list(), ["status", "--short", "--branch"]);
    }

    #[test]
    fn arg_if_missing_skips_existing() {
        let mut command = Command::new("git");
        command.arg("status");
        command.arg_if_missing("status");
        command.arg_if_missing("--short");

        assert_eq!(command.get_args_list(), ["status", "--short"]);
    }

    #[test]
    fn contains_arg_checks_quoted_and_raw() {
        let mut command = Command::new("git");
        command.arg(Arg {
            quoted_value: Some(OsString::from("'with space'")),
            value: OsString::from("with space"),
        });

        assert!(command.contains_arg("with space"));
        assert!(command.contains_arg("'with space'"));
        assert!(!command.contains_arg("other"));
    }
}

mod envs {
    use super::*;

    #[test]
    fn sets_and_unsets_vars() {
        let mut command = Command::new("git");
        command.env("SET", "1");
        command.env_opt("OPT_NONE", None::<&str>);
        command.env_remove("REMOVED");

        assert_eq!(
            command.env.get(OsStr::new("SET")),
            Some(&Env::Set(OsString::from("1")))
        );
        assert_eq!(command.env.get(OsStr::new("OPT_NONE")), Some(&Env::Unset));
        assert_eq!(command.env.get(OsStr::new("REMOVED")), Some(&Env::Unset));
    }

    #[test]
    fn sets_many_vars_at_once() {
        let mut command = Command::new("git");
        command.envs([("A", "1"), ("B", "2")]);
        command.envs_opt([("C", Some("3")), ("D", None::<&str>)]);
        command.envs_remove(["E", "F"]);

        assert_eq!(
            command.env.get(OsStr::new("A")),
            Some(&Env::Set(OsString::from("1")))
        );
        assert_eq!(
            command.env.get(OsStr::new("B")),
            Some(&Env::Set(OsString::from("2")))
        );
        assert_eq!(
            command.env.get(OsStr::new("C")),
            Some(&Env::Set(OsString::from("3")))
        );
        assert_eq!(command.env.get(OsStr::new("D")), Some(&Env::Unset));
        assert_eq!(command.env.get(OsStr::new("E")), Some(&Env::Unset));
        assert_eq!(command.env.get(OsStr::new("F")), Some(&Env::Unset));
    }

    #[test]
    fn later_values_overwrite_earlier_ones() {
        let mut command = Command::new("git");
        command.env("KEY", "first");
        command.env("KEY", "second");

        assert_eq!(
            command.env.get(OsStr::new("KEY")),
            Some(&Env::Set(OsString::from("second")))
        );

        command.env_remove("KEY");

        assert_eq!(command.env.get(OsStr::new("KEY")), Some(&Env::Unset));
    }

    #[test]
    fn inherit_colors_forces_a_terminal_size() {
        let mut command = Command::new("git");
        // Skip the color vars, as they depend on the ambient environment
        command.debug.is_test_env = true;
        command.inherit_colors();

        assert_eq!(
            command.env.get(OsStr::new("COLUMNS")),
            Some(&Env::Set(OsString::from("80")))
        );
        assert_eq!(
            command.env.get(OsStr::new("LINES")),
            Some(&Env::Set(OsString::from("24")))
        );
        assert!(!command.contains_env("FORCE_COLOR"));
    }

    #[test]
    fn inherit_colors_respects_explicit_vars() {
        let mut command = Command::new("git");
        command.env("NO_COLOR", "1");
        command.inherit_colors();

        assert_eq!(
            command.env.get(OsStr::new("NO_COLOR")),
            Some(&Env::Set(OsString::from("1")))
        );
        assert!(!command.contains_env("CLICOLOR_FORCE"));
    }

    #[test]
    fn contains_env_includes_unset_vars() {
        let mut command = Command::new("git");
        command.env("SET", "1");
        command.env_remove("REMOVED");

        assert!(command.contains_env("SET"));
        assert!(command.contains_env("REMOVED"));
        assert!(!command.contains_env("MISSING"));
    }
}

mod paths {
    use super::*;

    #[test]
    fn appends_and_prepends_in_order() {
        let mut command = Command::new("git");
        command.append_paths(["/c"]);
        command.prepend_paths(["/a", "/b"]);

        assert_eq!(
            command.paths,
            [
                OsString::from("/a"),
                OsString::from("/b"),
                OsString::from("/c")
            ]
        );
    }
}

mod bin_name {
    use super::*;

    #[test]
    fn returns_binary_name() {
        assert_eq!(Command::new("git").get_bin_name(), "git");
        assert_eq!(Command::new_bin("cargo").get_bin_name(), "cargo");
    }

    #[test]
    fn returns_first_word_of_script() {
        assert_eq!(
            Command::new_script("git commit --allow-empty").get_bin_name(),
            "git"
        );
        assert_eq!(Command::new_script("solo").get_bin_name(), "solo");
    }
}

mod exe {
    use super::*;

    #[test]
    fn replaces_the_binary() {
        let mut command = Command::new("git");
        command.set_bin("svn");

        assert_eq!(command.get_bin_name(), "svn");
        assert!(matches!(command.exe, Executable::Binary(_)));
    }

    #[test]
    fn replaces_the_script() {
        let mut command = Command::new("git");
        command.set_script("git status");

        assert_eq!(command.get_script(), "git status");
        assert!(matches!(command.exe, Executable::Script(_)));
    }

    #[test]
    fn new_bin_accepts_a_quoted_arg() {
        let command = Command::new_bin(Arg {
            quoted_value: Some(OsString::from("'my git'")),
            value: OsString::from("my git"),
        });

        assert_eq!(command.get_bin_name(), "my git");
    }
}

mod flags {
    use super::*;

    #[test]
    fn errors_on_nonzero_by_default() {
        let mut command = Command::new("git");

        assert!(command.should_error_nonzero());

        command.set_error_on_nonzero(false);

        assert!(!command.should_error_nonzero());
    }

    #[test]
    fn caches_output_only_when_enabled() {
        let mut command = Command::new("git");

        assert!(!command.should_cache_output());

        command.set_cache(true);

        assert!(command.should_cache_output());
    }

    #[test]
    fn never_caches_output_in_test_or_daemon_envs() {
        let mut command = Command::new("git");
        command.set_cache(true);
        command.debug.is_test_env = true;

        assert!(!command.should_cache_output());

        let mut command = Command::new("git");
        command.set_cache(true);
        command.debug.is_daemon_env = true;

        assert!(!command.should_cache_output());
    }

    #[test]
    fn tracks_the_remaining_toggles() {
        let mut command = Command::new("git");

        assert_eq!(command.get_prefix(), None);
        assert!(!command.continuous_pipe);
        assert!(!command.print_command);

        command.set_prefix("app:build");
        command.set_continuous_pipe(true);
        command.set_print_command(true);

        assert_eq!(command.get_prefix(), Some("app:build"));
        assert!(command.continuous_pipe);
        assert!(command.print_command);
    }
}

mod input {
    use super::*;

    #[test]
    fn tracks_input_and_size() {
        let mut command = Command::new("cat");

        assert!(!command.should_pass_stdin());

        command.input(["abc", "de"]);

        assert!(command.should_pass_stdin());
        assert_eq!(command.get_input_size(), 5);
    }

    #[test]
    fn accumulates_across_calls() {
        let mut command = Command::new("cat");
        command.input(["ab"]);
        command.input(["cd"]);

        assert_eq!(command.input.len(), 2);
        assert_eq!(command.get_input_size(), 4);
    }
}

mod shells {
    use super::*;

    #[test]
    fn no_shell_removes_and_set_script_restores() {
        let mut command = Command::new("git");
        command.no_shell();

        assert!(command.shell.is_none());

        command.set_script("git status");

        assert!(command.shell.is_some());
    }
}

mod cache_key {
    use super::*;

    #[test]
    fn is_stable_across_env_insertion_order() {
        let mut a = Command::new("git");
        a.env("A", "1").env("B", "2");

        let mut b = Command::new("git");
        b.env("B", "2").env("A", "1");

        assert_eq!(a.get_cache_key(), b.get_cache_key());
    }

    #[test]
    fn does_not_collide_on_adjacent_values() {
        let mut a = Command::new("git");
        a.args(["ab", "c"]);

        let mut b = Command::new("git");
        b.args(["a", "bc"]);

        assert_ne!(a.get_cache_key(), b.get_cache_key());

        let mut a = Command::new("git");
        a.env("FOO", "BAR");

        let mut b = Command::new("git");
        b.env("FOOB", "AR");

        assert_ne!(a.get_cache_key(), b.get_cache_key());
    }

    #[test]
    fn distinguishes_env_variants() {
        let mut set = Command::new("git");
        set.env("KEY", "1");

        let mut set_if_missing = Command::new("git");
        set_if_missing.env_with_behavior("KEY", Env::SetIfMissing(OsString::from("1")));

        let mut unset = Command::new("git");
        unset.env_remove("KEY");

        let absent = Command::new("git");

        assert_ne!(set.get_cache_key(), set_if_missing.get_cache_key());
        assert_ne!(set.get_cache_key(), unset.get_cache_key());
        assert_ne!(unset.get_cache_key(), absent.get_cache_key());
    }

    #[test]
    fn changes_with_exe_args_cwd_and_input() {
        let base = Command::new("git").get_cache_key();

        let mut with_arg = Command::new("git");
        with_arg.arg("status");

        let mut with_cwd = Command::new("git");
        with_cwd.cwd("/tmp");

        let mut with_input = Command::new("git");
        with_input.input(["data"]);

        assert_ne!(with_arg.get_cache_key(), base);
        assert_ne!(with_cwd.get_cache_key(), base);
        assert_ne!(with_input.get_cache_key(), base);
        assert_ne!(Command::new("svn").get_cache_key(), base);
    }

    #[test]
    fn changes_with_the_script() {
        assert_ne!(
            Command::new_script("git status").get_cache_key(),
            Command::new_script("git log").get_cache_key()
        );
    }

    #[test]
    fn ignores_fields_that_do_not_change_the_result() {
        let base = Command::new("git").get_cache_key();

        let mut command = Command::new("git");
        command.set_prefix("app:build");
        command.set_print_command(true);
        command.set_shell(ShellType::Bash);

        assert_eq!(command.get_cache_key(), base);
    }
}

mod command_line {
    use super::*;

    #[test]
    fn formats_binary_without_shell() {
        let mut command = Command::new("git");
        command.arg("status");

        assert_eq!(command.get_command_line(false, false), "git status");
    }

    #[test]
    fn formats_shell_wrapper_with_curly_quotes() {
        let mut command = Command::new("git");
        command.arg("status").set_shell(ShellType::Bash);

        let line = command.get_command_line(true, false);

        assert!(line.contains("-c “git status”"));
    }

    #[test]
    fn includes_input() {
        let mut command = Command::new("cat");
        command.no_shell();
        command.input(["abc"]);

        let line = command.get_command_line(true, true);

        assert!(line.ends_with("- abc"));
    }

    #[test]
    fn formats_scripts_inside_the_shell_wrapper() {
        let mut command = Command::new_script("git status && git log");
        command.set_shell(ShellType::Bash);

        let line = command.get_command_line(true, false);

        assert!(line.contains("-c \u{201c}git status && git log\u{201d}"));
    }

    #[test]
    fn wraps_scripts_even_without_a_shell() {
        // Scripts always require a shell, so one is used regardless
        let mut command = Command::new_script("git status");
        command.no_shell();

        assert!(command.get_command_line(true, false).contains("-c"));
    }

    #[test]
    fn collapses_newlines() {
        let command = Command::new_script("git status\ngit log");

        assert!(!command.get_command_line(false, false).contains('\n'));
    }

    #[test]
    fn omits_input_when_not_requested() {
        let mut command = Command::new("cat");
        command.no_shell();
        command.input(["abc"]);

        assert_eq!(command.get_command_line(true, false), "cat");
    }

    #[test]
    fn truncates_large_input() {
        let mut command = Command::new("cat");
        command.no_shell();
        command.input(["x".repeat(250)]);

        let line = command.get_command_line(true, true);

        assert!(line.contains("(truncated input, 250 total bytes)"));
    }

    #[test]
    fn debug_flag_prints_large_input() {
        let mut command = Command::new("cat");
        command.no_shell();
        command.debug.print_input = true;
        command.input(["x".repeat(250)]);

        let line = command.get_command_line(true, true);

        assert!(!line.contains("truncated input"));
        assert!(line.ends_with(&"x".repeat(250)));
    }
}

mod scripts {
    use super::*;

    #[test]
    fn returns_full_script() {
        assert_eq!(
            Command::new_script("git commit --allow-empty").get_script(),
            "git commit --allow-empty"
        );
        assert_eq!(Command::new("git").get_script(), "git");
    }
}

mod console {
    use super::*;
    use starbase_console::{Console, Reporter};

    #[derive(Debug, Default)]
    struct OtherReporter;

    impl Reporter for OtherReporter {}

    #[test]
    fn attaches_a_console() {
        let mut command = Command::new("git");

        assert!(command.console.is_none());

        command.set_console(Console::new_testing());

        assert!(command.console.is_some());
    }

    #[test]
    fn with_console_swaps_the_reporter_and_keeps_the_command() {
        let mut command = Command::new("git");
        command.arg("status").env("KEY", "value").cwd("/tmp");

        let mut console: Console<OtherReporter> = Console::new_testing();
        console.set_reporter(OtherReporter);

        let converted: starbase_process::Command<OtherReporter> = command.with_console(console);

        assert_eq!(converted.get_args_list(), ["status"]);
        assert!(converted.contains_env("KEY"));
        assert_eq!(converted.cwd, Some(OsString::from("/tmp")));
        assert!(converted.console.is_some());
    }
}
