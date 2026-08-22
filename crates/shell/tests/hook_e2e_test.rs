// End-to-end tests for the `Hook::OnChangeDir` activation flow.
//
// Each test renders the hook with a stub "activate" command, runs the result
// through the real shell binary, and asserts that: the emitted statements are
// evaluated and environment changes propagate to the session, the change-dir
// trigger fires, and sourcing the hook twice registers it once.
//
// A test is skipped when its shell is not installed, unless the shell is
// listed in the `STARBASE_REQUIRED_SHELLS` environment variable
// (comma-separated binary names), which CI sets per operating system so a
// broken install step fails loudly instead of silently skipping.

use starbase_sandbox::{Sandbox, create_empty_sandbox};
use starbase_shell::{Hook, ShellType};
use std::io;
use std::process::{Command, Output};

fn format_hook(shell: ShellType, command: &str) -> String {
    shell
        .build()
        .format_hook(Hook::OnChangeDir {
            command: command.into(),
            function: "_starbase_hook".into(),
        })
        .unwrap()
}

fn run_script(sandbox: &Sandbox, bin: &str, args: &[&str]) -> Option<Output> {
    match Command::new(bin)
        .args(args)
        .current_dir(sandbox.path())
        .output()
    {
        Ok(output) => {
            assert!(
                output.status.success(),
                "{bin} exited with {:?}\nstdout:\n{}\nstderr:\n{}",
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );

            Some(output)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let required = std::env::var("STARBASE_REQUIRED_SHELLS").unwrap_or_default();

            if required.split(',').any(|name| name.trim() == bin) {
                panic!("{bin} is required for E2E tests but was not found on PATH");
            }

            println!("{bin} not found on PATH, skipping");

            None
        }
        Err(error) => panic!("failed to execute {bin}: {error}"),
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n")
}

#[cfg(unix)]
#[test]
fn bash_applies_exports_and_registers_once() {
    let hook = format_hook(
        ShellType::Bash,
        r#"printf 'export E2E_FOO="123";\nexport E2E_BAR="456";\n'"#,
    );

    let sandbox = create_empty_sandbox();
    sandbox.create_file("hook.sh", &hook);

    // String form `PROMPT_COMMAND`: invoking the hook (as the prompt would)
    // applies the exports, and sourcing twice registers once.
    sandbox.create_file(
        "string.sh",
        r#"
PROMPT_COMMAND="starship_precmd"
source ./hook.sh
source ./hook.sh
_starbase_hook
printf '%s\n' "$PROMPT_COMMAND"
printf 'E2E_FOO=%s E2E_BAR=%s\n' "$E2E_FOO" "$E2E_BAR"
"#,
    );

    if let Some(output) = run_script(&sandbox, "bash", &["./string.sh"]) {
        assert_eq!(
            stdout(&output),
            "_starbase_hook;starship_precmd\nE2E_FOO=123 E2E_BAR=456\n"
        );
    }

    // Array form `PROMPT_COMMAND` (bash >= 5.1) with multiple entries:
    // sourcing twice must register once.
    sandbox.create_file(
        "array.sh",
        r#"
PROMPT_COMMAND=(starship_precmd other_thing)
source ./hook.sh
source ./hook.sh
printf '%s\n' "${PROMPT_COMMAND[*]}"
"#,
    );

    if let Some(output) = run_script(&sandbox, "bash", &["./array.sh"]) {
        assert_eq!(
            stdout(&output),
            "_starbase_hook starship_precmd other_thing\n"
        );
    }
}

#[cfg(unix)]
#[test]
fn zsh_fires_on_cd_and_registers_once() {
    let hook = format_hook(
        ShellType::Zsh,
        r#"printf 'export E2E_FOO="123";\nexport E2E_BAR="456";\n'"#,
    );

    let sandbox = create_empty_sandbox();
    sandbox.create_file("hook.zsh", &hook);
    sandbox.create_file(
        "test.zsh",
        r#"
source ./hook.zsh
cd /
print -r -- "E2E_FOO=$E2E_FOO E2E_BAR=$E2E_BAR"
source ./hook.zsh
print -r -- "HOOKS=${#chpwd_functions[@]}"
"#,
    );

    // -f skips rc files so user config cannot interfere
    if let Some(output) = run_script(&sandbox, "zsh", &["-f", "./test.zsh"]) {
        assert_eq!(stdout(&output), "E2E_FOO=123 E2E_BAR=456\nHOOKS=1\n");
    }
}

#[cfg(unix)]
#[test]
fn fish_fires_on_cd() {
    let hook = format_hook(
        ShellType::Fish,
        r#"printf 'set -gx E2E_FOO 123;\nset -gx E2E_BAR 456;\n'"#,
    );

    let sandbox = create_empty_sandbox();
    sandbox.create_file("hook.fish", &hook);
    sandbox.create_file(
        "test.fish",
        r#"
source ./hook.fish
cd /
printf 'E2E_FOO=%s E2E_BAR=%s\n' $E2E_FOO $E2E_BAR
"#,
    );

    if let Some(output) = run_script(&sandbox, "fish", &["--no-config", "./test.fish"]) {
        assert_eq!(stdout(&output), "E2E_FOO=123 E2E_BAR=456\n");
    }
}

#[cfg(unix)]
#[test]
fn elvish_evals_multiline_output_and_fires_on_cd() {
    let hook = format_hook(
        ShellType::Elvish,
        // Multi-line on purpose: elvish output capture splits byte output
        // into one value per line, which `slurp` must collapse
        r#"print "set-env E2E_FOO 123;\nset-env E2E_BAR 456;\n""#,
    );

    let sandbox = create_empty_sandbox();

    // Elvish has no `source`, so concatenate the hook and the assertions.
    // The bare call mirrors the init invocation consumers append (no args),
    // while `cd` invokes it through `$after-chdir` (one arg).
    sandbox.create_file(
        "test.elv",
        format!(
            "{hook}\n{}",
            r#"
_starbase_hook
echo direct $E:E2E_FOO
set-env E2E_FOO reset
cd /
echo chdir $E:E2E_FOO $E:E2E_BAR
"#
        ),
    );

    if let Some(output) = run_script(&sandbox, "elvish", &["./test.elv"]) {
        assert_eq!(stdout(&output), "direct 123\nchdir 123 456\n");
    }
}

// Nushell cannot eval code at runtime, so the hook parses a JSON payload
// instead: `{ env: { KEY: value | null }, paths: [..], path: ".." }`. Hooks
// only fire inside the interactive REPL, so this drives the hook function
// directly (as the init call does) and asserts the registration instead.
#[test]
fn nu_applies_json_payload_and_registers() {
    let hook = format_hook(
        ShellType::Nu,
        r#"echo '{"env":{"E2E_FOO":"123","E2E_GONE":null},"paths":[],"path":"/e2e-stub-path"}'"#,
    );

    let path_key = if cfg!(windows) { "Path" } else { "PATH" };

    let sandbox = create_empty_sandbox();
    sandbox.create_file(
        "test.nu",
        format!(
            r#"$env.E2E_GONE = "preset"

{hook}

_starbase_hook

print $"foo=($env | get --optional E2E_FOO | default MISSING)"
print $"gone=($env | get --optional E2E_GONE | default REMOVED)"
print $"path=($env.{path_key})"
print $"hooks=($env.config.hooks.env_change.PWD | length)"
"#
        ),
    );

    if let Some(output) = run_script(&sandbox, "nu", &["./test.nu"]) {
        assert_eq!(
            stdout(&output),
            "foo=123\ngone=REMOVED\npath=/e2e-stub-path\nhooks=1\n"
        );
    }
}

// The murex `onPrompt` event only fires interactively, so this drives the
// hook function directly, and also asserts the `!event` removal syntax that
// deactivation will rely on.
#[cfg(unix)]
#[test]
fn murex_applies_exports_and_event_is_removable() {
    let hook = format_hook(ShellType::Murex, r#"out "export E2E_FOO=123""#);

    let sandbox = create_empty_sandbox();
    sandbox.create_file(
        "test.mx",
        format!(
            "{hook}\n{}",
            r#"
_starbase_hook
out $ENV.E2E_FOO
!event onPrompt _starbase_hook
out unregistered
"#
        ),
    );

    if let Some(output) = run_script(&sandbox, "murex", &["./test.mx"]) {
        assert_eq!(stdout(&output), "123\nunregistered\n");
    }
}

#[test]
fn pwsh_fires_on_cd_registers_once_and_preserves_exit_code() {
    // The exports themselves run a native command (exit 3) to prove the hook
    // restores the `$LASTEXITCODE` that was live before it fired.
    let hook = format_hook(
        ShellType::Pwsh,
        r#"Write-Output '& pwsh -NoProfile -Command ''exit 3''; $env:E2E_FOO = "123"; $env:E2E_BAR = "456";'"#,
    );

    let sandbox = create_empty_sandbox();
    sandbox.create_file("hook.ps1", &hook);
    sandbox.create_file(
        "test.ps1",
        r#"
. ./hook.ps1
. ./hook.ps1
& pwsh -NoProfile -Command 'exit 7'
Set-Location ([System.IO.Path]::GetTempPath())
Write-Output "E2E_FOO=$env:E2E_FOO E2E_BAR=$env:E2E_BAR"
Write-Output "EXIT=$global:LASTEXITCODE"
Write-Output "HANDLERS=$($ExecutionContext.SessionState.InvokeCommand.LocationChangedAction.GetInvocationList().Count)"
"#,
    );

    if let Some(output) = run_script(&sandbox, "pwsh", &["-NoProfile", "-File", "./test.ps1"]) {
        assert_eq!(
            stdout(&output),
            "E2E_FOO=123 E2E_BAR=456\nEXIT=7\nHANDLERS=1\n"
        );
    }
}
