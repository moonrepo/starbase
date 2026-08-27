use serial_test::serial;
use starbase_sandbox::assert_snapshot;
use starbase_shell::{Hook, ShellType};
use std::env;

fn hooks(shell_type: ShellType) -> [Hook; 3] {
    [
        Hook::Activate {
            command: format!("starbase hook {shell_type}"),
            function: "_starbase_activate".into(),
        },
        Hook::Deactivate {
            command: format!("starbase deactivate {shell_type}"),
            function: "_starbase_deactivate".into(),
        },
        Hook::OnContextChange {
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
