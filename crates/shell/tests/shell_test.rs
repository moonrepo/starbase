use serial_test::serial;
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
