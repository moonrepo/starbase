use serial_test::serial;
use starbase_sandbox::assert_snapshot;
use starbase_shell::{Hook, ShellType};
use std::env;

#[test]
fn all_shells_except_ion_support_context_change_hook() {
    for shell_type in ShellType::variants() {
        let result = shell_type.build().format_hook(Hook::OnContextChange {
            activate_command: format!("starbase hook {shell_type}"),
            activate_function: "_starbase_hook".into(),
            deactivate_command: format!("starbase deactivate {shell_type}"),
            deactivate_function: "_starbase_deactivate".into(),
        });

        if shell_type == ShellType::Ion {
            assert!(
                result.is_err(),
                "{shell_type} should not support context change hooks"
            );
        } else {
            assert!(
                result.is_ok(),
                "{shell_type} should support context change hooks"
            );
        }
    }
}

// The hook templates are rendered at runtime, so a misspelled or renamed
// placeholder cannot fail the build the way `format!` arguments did. Every
// unresolved placeholder survives into the output, so scan for one.
#[test]
fn no_shell_leaves_template_placeholders() {
    for shell_type in ShellType::variants() {
        let Ok(output) = shell_type.build().format_hook(Hook::OnContextChange {
            activate_command: format!("starbase hook {shell_type}"),
            activate_function: "_starbase_hook".into(),
            deactivate_command: format!("starbase deactivate {shell_type}"),
            deactivate_function: "_starbase_deactivate".into(),
        }) else {
            continue;
        };

        assert!(
            !output.contains("${{"),
            "{shell_type} hook has an unresolved template placeholder:\n{output}"
        );
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
