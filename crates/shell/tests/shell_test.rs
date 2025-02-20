use serial_test::serial;
use starbase_shell::ShellType;
use std::env;

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

    assert!(ShellType::detect().is_some());
}
