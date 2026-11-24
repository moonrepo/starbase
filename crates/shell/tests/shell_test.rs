use serial_test::serial;
use starbase_sandbox::assert_snapshot;
use starbase_shell::{Hook, ShellType};
use std::env;

fn hooks(shell_type: ShellType) -> [Hook; 4] {
    [
        Hook::Activate {
            command: format!("starbase hook {shell_type}"),
            function: "_starbase_activate".into(),
        },
        Hook::Deactivate {
            command: format!("starbase deactivate {shell_type}"),
            function: "_starbase_deactivate".into(),
        },
        Hook::RegisterHandlers {
            function: "_starbase_activate".into(),
        },
        Hook::UnregisterHandlers {
            function: "_starbase_activate".into(),
        },
    ]
}

#[test]
fn all_shells_except_ion_support_every_hook() {
    for shell_type in ShellType::variants() {
        for hook in hooks(shell_type) {
            let info = hook.get_info().to_owned();
            let result = shell_type.build().format_hook(hook);

            if shell_type == ShellType::Ion {
                assert!(result.is_err(), "{shell_type} should not support {info}");
            } else {
                assert!(result.is_ok(), "{shell_type} should support {info}");
            }
        }
    }
}

// Deactivating is activating with another name and another command, so the two
// share a template. Anything that drifts between them is a bug in the shell.
#[test]
fn deactivate_matches_activate() {
    for shell_type in ShellType::variants() {
        let shell = shell_type.build();

        let Ok(activate) = shell.format_hook(Hook::Activate {
            command: "print statements".into(),
            function: "_starbase_hook".into(),
        }) else {
            continue;
        };

        let deactivate = shell
            .format_hook(Hook::Deactivate {
                command: "print statements".into(),
                function: "_starbase_hook".into(),
            })
            .unwrap();

        assert_eq!(activate, deactivate, "{shell_type}");
    }
}

// The hook templates are rendered at runtime, so a misspelled or renamed
// placeholder cannot fail the build the way `format!` arguments did. Every
// unresolved placeholder survives into the output, so scan for one.
#[test]
fn no_shell_leaves_template_placeholders() {
    for shell_type in ShellType::variants() {
        for hook in hooks(shell_type) {
            let info = hook.get_info().to_owned();

            let Ok(output) = shell_type.build().format_hook(hook) else {
                continue;
            };

            assert!(
                !output.contains("${{"),
                "{shell_type} {info} hook has an unresolved template placeholder:\n{output}"
            );
        }
    }
}

// A plain alphanumeric value needs no shell quoting, and that is exactly the
// case that broke three shells: nu and murex parse the right side of an
// assignment as an expression, powershell as a pipeline, and a bareword is a
// command there, not a value.
#[test]
fn formats_a_plain_env_value_for_every_shell() {
    let rendered = ShellType::variants()
        .iter()
        .map(|shell_type| {
            format!(
                "{shell_type}: {}",
                shell_type.build().format_env_set("KEY", "value")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert_snapshot!(rendered);
}

#[test]
fn expression_shells_quote_a_plain_env_value() {
    for shell_type in [
        ShellType::Murex,
        ShellType::Nu,
        ShellType::PowerShell,
        ShellType::Pwsh,
    ] {
        let output = shell_type.build().format_env_set("KEY", "value");

        assert!(
            ["'value'", "\"value\"", "%(value)"]
                .iter()
                .any(|quoted| output.contains(quoted)),
            "{shell_type} left a plain value unquoted: {output}"
        );
    }
}

#[test]
#[serial]
fn detects_a_shell_with_env_var() {
    unsafe { env::set_var("SHELL", "zsh") };

    assert_eq!(ShellType::detect().unwrap(), ShellType::Zsh);
}

#[test]
#[serial]
fn detects_a_shell_from_os() {
    unsafe { env::remove_var("SHELL") };

    assert!(ShellType::os_variants().contains(&ShellType::detect_with_fallback()));
}

mod resolve_shell_program {
    use serial_test::serial;
    use starbase_sandbox::create_empty_sandbox;
    use starbase_shell::{
        ShellType, find_executable_in_path, find_shell_in_path, find_shell_path,
        resolve_shell_program, set_skipped_shell_dirs,
    };
    use std::env::{self, consts::EXE_SUFFIX};
    use std::fs;
    use std::path::{Path, PathBuf};

    fn create_exe(dir: &Path, name: &str, contents: &str) -> PathBuf {
        let path = dir.join(format!("{name}{EXE_SUFFIX}"));

        fs::create_dir_all(dir).unwrap();
        fs::write(&path, contents).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }

        path
    }

    #[test]
    fn finds_the_first_match() {
        let sandbox = create_empty_sandbox();
        let a = sandbox.path().join("a");
        let b = sandbox.path().join("b");
        let c = sandbox.path().join("c");

        fs::create_dir_all(&a).unwrap();
        let expected = create_exe(&b, "tool", "");
        create_exe(&c, "tool", "");

        let path = env::join_paths([a, b, c]).unwrap();

        assert_eq!(find_executable_in_path("tool", &path), Some(expected));
    }

    #[test]
    fn returns_none_when_missing() {
        let sandbox = create_empty_sandbox();
        let path = env::join_paths([sandbox.path()]).unwrap();

        assert_eq!(find_executable_in_path("tool", &path), None);
    }

    #[cfg(unix)]
    #[test]
    fn skips_files_that_arent_executable() {
        let sandbox = create_empty_sandbox();
        let a = sandbox.path().join("a");
        let b = sandbox.path().join("b");

        fs::create_dir_all(&a).unwrap();
        fs::write(a.join("tool"), "").unwrap();
        let expected = create_exe(&b, "tool", "");

        let path = env::join_paths([a, b]).unwrap();

        assert_eq!(find_executable_in_path("tool", &path), Some(expected));
    }

    fn find_shell_with_skipped_dirs(dirs: &[&Path], path: &std::ffi::OsStr) -> Option<PathBuf> {
        set_skipped_shell_dirs(dirs);
        let found = find_shell_in_path("tool", path);
        set_skipped_shell_dirs(Vec::<PathBuf>::new());
        found
    }

    // Like a dependency's `bash` in a workspace's `node_modules/.bin`,
    // which a package manager added to `PATH` before running a program
    #[test]
    #[serial]
    fn skips_dirs_within_skipped_dirs() {
        let sandbox = create_empty_sandbox();
        let workspace = sandbox.path().join("workspace");
        let bin_dir = workspace.join("node_modules/.bin");
        let sys_dir = sandbox.path().join("sys");

        let skipped = create_exe(&bin_dir, "tool", "");
        let expected = create_exe(&sys_dir, "tool", "");

        let path = env::join_paths([&bin_dir, &sys_dir]).unwrap();

        assert_eq!(
            find_shell_with_skipped_dirs(&[&workspace], &path),
            Some(expected)
        );

        // Only when locating a shell
        assert_eq!(find_executable_in_path("tool", &path), Some(skipped));
    }

    #[test]
    #[serial]
    fn falls_back_to_skipped_dirs() {
        let sandbox = create_empty_sandbox();
        let workspace = sandbox.path().join("workspace");
        let bin_dir = workspace.join("node_modules/.bin");
        let sys_dir = sandbox.path().join("sys");

        fs::create_dir_all(&sys_dir).unwrap();
        let expected = create_exe(&bin_dir, "tool", "");

        let path = env::join_paths([&bin_dir, &sys_dir]).unwrap();

        assert_eq!(
            find_shell_with_skipped_dirs(&[&workspace], &path),
            Some(expected)
        );
    }

    #[cfg(unix)]
    #[test]
    #[serial]
    fn skips_the_real_path_of_skipped_dirs() {
        let sandbox = create_empty_sandbox();
        let workspace = sandbox.path().join("workspace");
        let link = sandbox.path().join("link");
        let sys_dir = sandbox.path().join("sys");

        create_exe(&workspace.join("node_modules/.bin"), "tool", "");
        let expected = create_exe(&sys_dir, "tool", "");
        std::os::unix::fs::symlink(&workspace, &link).unwrap();

        let path = env::join_paths([
            workspace.canonicalize().unwrap().join("node_modules/.bin"),
            sys_dir,
        ])
        .unwrap();

        assert_eq!(
            find_shell_with_skipped_dirs(&[&link], &path),
            Some(expected)
        );
    }

    #[cfg(unix)]
    #[test]
    #[serial]
    fn skips_path_dirs_through_a_symlink() {
        let sandbox = create_empty_sandbox();
        let workspace = sandbox.path().join("workspace");
        let link = sandbox.path().join("link");
        let sys_dir = sandbox.path().join("sys");

        create_exe(&workspace.join("node_modules/.bin"), "tool", "");
        let expected = create_exe(&sys_dir, "tool", "");
        std::os::unix::fs::symlink(&workspace, &link).unwrap();

        let path = env::join_paths([link.join("node_modules/.bin"), sys_dir]).unwrap();

        assert_eq!(
            find_shell_with_skipped_dirs(&[&workspace.canonicalize().unwrap()], &path),
            Some(expected)
        );
    }

    #[test]
    fn falls_back_to_the_name_when_not_found() {
        assert_eq!(
            resolve_shell_program("starbase-missing-shell"),
            "starbase-missing-shell"
        );
    }

    // A `bash` within a directory that's prepended to the command's own `PATH`,
    // like `node_modules/.bin`, must not replace the shell that wraps the command.
    #[cfg(unix)]
    #[test]
    fn wrapped_command_isnt_replaced_by_a_shell_on_its_own_path() {
        if find_shell_path(ShellType::Bash).is_none() {
            return;
        }

        let sandbox = create_empty_sandbox();
        let bin_dir = sandbox.path().join("node_modules/.bin");
        create_exe(&bin_dir, "bash", "#!/bin/sh\necho hijacked\n");

        let mut command = ShellType::Bash.build().create_wrapped_command("echo real");
        let mut paths = vec![bin_dir];
        paths.extend(env::split_paths(&env::var_os("PATH").unwrap()));
        command.env("PATH", env::join_paths(paths).unwrap());

        assert!(Path::new(command.get_program()).is_absolute());

        let output = command.output().unwrap();

        assert_eq!(String::from_utf8_lossy(&output.stdout), "real\n");
    }
}
