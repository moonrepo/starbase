// End-to-end tests for the `Hook::OnChangeDir` activation flow. Each test
// renders the hook with stub "activate" and "deactivate" commands, runs the
// result through the real shell binary, and asserts that:
//
// - the emitted activation statements are evaluated and environment changes
//   propagate to the session,
// - the change-dir trigger fires,
// - sourcing the hook twice registers it once,
// - the deactivate function reverts the environment, unregisters the trigger
//   (a subsequent cd does not re-activate), and removes the functions where
//   the shell allows it,
// - evaluating the hook again after deactivation re-activates: the trigger
//   re-registers (exactly once) and fires again.
//
// A test is skipped when its shell is not installed, unless the shell is
// listed in the `STARBASE_REQUIRED_SHELLS` environment variable
// (comma-separated binary names), which CI sets per operating system so a
// broken install step fails loudly instead of silently skipping.

use starbase_sandbox::{Sandbox, create_empty_sandbox};
use starbase_shell::{Hook, ShellType};
use std::io;
use std::process::{Command, Output};

fn format_hook(shell: ShellType, activate_command: &str, deactivate_command: &str) -> String {
    shell
        .build()
        .format_hook(Hook::OnChangeDir {
            activate_command: activate_command.into(),
            activate_function: "_starbase_activate".into(),
            deactivate_command: deactivate_command.into(),
            deactivate_function: "_starbase_deactivate".into(),
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
fn bash_activates_and_deactivates() {
    let hook = format_hook(
        ShellType::Bash,
        r#"printf 'export E2E_FOO="123";\nexport E2E_BAR="456";\n'"#,
        r#"printf 'unset E2E_FOO;\nunset E2E_BAR;\n'"#,
    );

    let sandbox = create_empty_sandbox();
    sandbox.create_file("hook.sh", &hook);

    // String form `PROMPT_COMMAND`: invoking the hook (as the prompt would)
    // applies the exports, sourcing twice registers once, and deactivating
    // reverts the environment and removes the registration + functions.
    sandbox.create_file(
        "string.sh",
        r#"
PROMPT_COMMAND="starship_precmd"
source ./hook.sh
source ./hook.sh
_starbase_activate
printf '%s\n' "$PROMPT_COMMAND"
printf 'E2E_FOO=%s E2E_BAR=%s\n' "$E2E_FOO" "$E2E_BAR"
_starbase_deactivate
printf '%s\n' "$PROMPT_COMMAND"
printf 'E2E_FOO=%s E2E_BAR=%s\n' "${E2E_FOO:-unset}" "${E2E_BAR:-unset}"
type -t _starbase_activate >/dev/null 2>&1 || printf 'functions removed\n'
source ./hook.sh
_starbase_activate
printf '%s\n' "$PROMPT_COMMAND"
printf 'E2E_FOO=%s\n' "$E2E_FOO"
"#,
    );

    if let Some(output) = run_script(&sandbox, "bash", &["./string.sh"]) {
        assert_eq!(
            stdout(&output),
            "_starbase_activate;starship_precmd\nE2E_FOO=123 E2E_BAR=456\nstarship_precmd\nE2E_FOO=unset E2E_BAR=unset\nfunctions removed\n_starbase_activate;starship_precmd\nE2E_FOO=123\n"
        );
    }

    // Array form `PROMPT_COMMAND` (bash >= 5.1) with multiple entries:
    // registration and deactivation only touch our own entry.
    sandbox.create_file(
        "array.sh",
        r#"
PROMPT_COMMAND=(starship_precmd other_thing)
source ./hook.sh
source ./hook.sh
printf '%s\n' "${PROMPT_COMMAND[*]}"
_starbase_deactivate
printf '%s\n' "${PROMPT_COMMAND[*]}"
source ./hook.sh
printf '%s\n' "${PROMPT_COMMAND[*]}"
"#,
    );

    if let Some(output) = run_script(&sandbox, "bash", &["./array.sh"]) {
        assert_eq!(
            stdout(&output),
            "_starbase_activate starship_precmd other_thing\nstarship_precmd other_thing\n_starbase_activate starship_precmd other_thing\n"
        );
    }
}

#[cfg(unix)]
#[test]
fn zsh_activates_and_deactivates() {
    let hook = format_hook(
        ShellType::Zsh,
        r#"printf 'export E2E_FOO="123";\nexport E2E_BAR="456";\n'"#,
        r#"printf 'unset E2E_FOO;\nunset E2E_BAR;\n'"#,
    );

    let sandbox = create_empty_sandbox();
    sandbox.create_file("hook.zsh", &hook);
    sandbox.create_file(
        "test.zsh",
        r#"
root="$(pwd)"
source "$root/hook.zsh"
cd /
print -r -- "E2E_FOO=$E2E_FOO E2E_BAR=$E2E_BAR"
source "$root/hook.zsh"
print -r -- "HOOKS=${#chpwd_functions[@]},${#precmd_functions[@]}"
_starbase_deactivate
print -r -- "HOOKS=${#chpwd_functions[@]},${#precmd_functions[@]}"
print -r -- "E2E_FOO=${E2E_FOO:-unset} E2E_BAR=${E2E_BAR:-unset}"
cd /tmp
print -r -- "after cd E2E_FOO=${E2E_FOO:-unset}"
whence -w _starbase_activate >/dev/null 2>&1 || print -r -- "functions removed"
source "$root/hook.zsh"
cd /usr
print -r -- "HOOKS=${#chpwd_functions[@]},${#precmd_functions[@]} E2E_FOO=${E2E_FOO:-unset}"
"#,
    );

    // -f skips rc files so user config cannot interfere
    if let Some(output) = run_script(&sandbox, "zsh", &["-f", "./test.zsh"]) {
        assert_eq!(
            stdout(&output),
            "E2E_FOO=123 E2E_BAR=456\nHOOKS=1,1\nHOOKS=0,0\nE2E_FOO=unset E2E_BAR=unset\nafter cd E2E_FOO=unset\nfunctions removed\nHOOKS=1,1 E2E_FOO=123\n"
        );
    }
}

#[cfg(unix)]
#[test]
fn fish_activates_and_deactivates() {
    let hook = format_hook(
        ShellType::Fish,
        r#"printf 'set -gx E2E_FOO 123;\nset -gx E2E_BAR 456;\n'"#,
        r#"printf 'set -ge E2E_FOO;\nset -ge E2E_BAR;\n'"#,
    );

    let sandbox = create_empty_sandbox();
    sandbox.create_file("hook.fish", &hook);
    sandbox.create_file(
        "test.fish",
        r#"
function handlers
  printf 'handlers=%s,%s\n' \
    (functions --handlers-type variable | string match -r '^PWD _starbase_activate$' | count) \
    (functions --handlers-type generic | string match -r '^fish_prompt _starbase_activate$' | count)
end

set root (pwd)
source $root/hook.fish
cd /
printf 'E2E_FOO=%s E2E_BAR=%s\n' $E2E_FOO $E2E_BAR
handlers
_starbase_deactivate
handlers
cd /tmp
if set -q E2E_FOO
  printf 'still set\n'
else
  printf 'E2E_FOO unset\n'
end
functions -q _starbase_activate; or printf 'functions removed\n'
source $root/hook.fish
cd /usr
printf 'E2E_FOO=%s\n' $E2E_FOO
"#,
    );

    if let Some(output) = run_script(&sandbox, "fish", &["--no-config", "./test.fish"]) {
        assert_eq!(
            stdout(&output),
            "E2E_FOO=123 E2E_BAR=456\nhandlers=1,1\nhandlers=0,0\nE2E_FOO unset\nfunctions removed\nE2E_FOO=123\n"
        );
    }
}

#[cfg(unix)]
#[test]
fn elvish_activates_and_deactivates() {
    let hook = format_hook(
        ShellType::Elvish,
        // Multi-line on purpose: elvish output capture splits byte output
        // into one value per line, which `slurp` must collapse
        r#"print "set-env E2E_FOO 123;\nset-env E2E_BAR 456;\n""#,
        r#"print "unset-env E2E_FOO;\nunset-env E2E_BAR;\n""#,
    );

    let sandbox = create_empty_sandbox();
    sandbox.create_file("hook.elv", &hook);

    // Elvish has no `source`, so concatenate the hook and the assertions.
    // The bare call mirrors the init invocation consumers append (no args),
    // while `cd` invokes it through `$after-chdir` (one arg). Deactivation
    // unregisters but cannot delete the functions. Re-activation must go
    // through `eval` (a separate compile unit), since redefining a `fn`
    // within the same unit is a compile error — this mirrors a session
    // re-evaluating the activation output.
    //
    // The hook exports both functions to the interactive namespace with
    // `edit:add-vars`, drops them again with `edit:del-vars`, and registers
    // the prompt trigger on `$edit:before-readline`. The `edit:` module
    // doesn't exist when scripted, so this run also asserts that the `try`
    // around each call swallows that and nothing else aborts — under `eval`
    // an uncaught exception there would take the whole hook down. That also
    // means only the `$after-chdir` trigger is observable here.
    sandbox.create_file(
        "test.elv",
        format!(
            "{hook}\n{}",
            r#"
var root = $pwd
eval (slurp < $root/hook.elv)
_starbase_activate
echo direct=$E:E2E_FOO
set-env E2E_FOO reset
cd /
echo chdir=$E:E2E_FOO,$E:E2E_BAR
echo hooks=(count $after-chdir)
_starbase_deactivate
echo hooks=(count $after-chdir)
echo removed=$E:E2E_FOO
set-env E2E_FOO reset2
cd /tmp
echo after=$E:E2E_FOO
eval (slurp < $root/hook.elv)
cd /usr
echo cycle=$E:E2E_FOO hooks=(count $after-chdir)
"#
        ),
    );

    if let Some(output) = run_script(&sandbox, "elvish", &["./test.elv"]) {
        assert_eq!(
            stdout(&output),
            "direct=123\nchdir=123,456\nhooks=1\nhooks=0\nremoved=\nafter=reset2\ncycle=123 hooks=1\n"
        );
    }
}

// Nushell cannot eval code at runtime, so both hook functions parse a JSON
// payload instead: `{ env: { KEY: value | null }, paths: [..], path: ".." }`.
// Hooks only fire inside the interactive REPL, so this drives the functions
// directly (as the init call does) and asserts the registration list instead.
#[test]
fn nu_activates_and_deactivates() {
    let hook = format_hook(
        ShellType::Nu,
        r#"echo '{"env":{"E2E_FOO":"123","E2E_GONE":null},"paths":[],"path":"/e2e-stub-path","aliases":{"e2e_ll":"echo ALIASOK","e2e_gone":null}}'"#,
        r#"echo '{"env":{"E2E_FOO":null},"paths":[],"path":"/e2e-restored-path","aliases":{"e2e_ll":null}}'"#,
    );

    let path_key = if cfg!(windows) { "Path" } else { "PATH" };

    let sandbox = create_empty_sandbox();
    sandbox.create_file("hook.nu", &hook);

    // Sourcing the same file twice dedupes the `export def`s at parse time,
    // but re-runs `export-env` — so the second source exercises the
    // registration dedup guard, and the source after deactivation re-registers.
    //
    // Aliases are staged as a `pre_prompt` hook, since only a hook defined as
    // a string is parsed in the scope that triggered it. Hooks never fire in a
    // script, so this asserts the staged definitions instead: what they are,
    // and that feeding them to a fresh nu really does define the alias.
    sandbox.create_file(
        "test.nu",
        format!(
            r#"$env.E2E_GONE = "preset"

def registered [] {{
    let hook = {{ code: "_starbase_activate" }}
    let pwd_list = ($env.config | get --optional hooks.env_change.PWD) | default []
    let prompt_list = ($env.config | get --optional hooks.pre_prompt) | default []

    $"($pwd_list | where {{ |it| $it == $hook }} | length),($prompt_list | where {{ |it| $it == $hook }} | length)"
}}

def staged [] {{
    $env.config.hooks.pre_prompt | last | get code
}}

def staged_count [] {{
    $env.config.hooks.pre_prompt
        | where {{ |it| ($it | describe | str starts-with "record") and (($it | get --optional code | default "") | str contains "_starbase_activate aliases") }}
        | length
}}

def staged_defs [] {{
    staged | lines | where {{ |line| ($line | str starts-with "alias ") or ($line | str starts-with "hide ") }} | str join ","
}}

source "./hook.nu"
source "./hook.nu"

_starbase_activate

print $"foo=($env | get --optional E2E_FOO | default MISSING)"
print $"gone=($env | get --optional E2E_GONE | default REMOVED)"
print $"path=($env.{path_key})"
print $"hooks=(registered)"
print $"staged=(staged_count) (staged_defs)"
print $"parsed=(^$nu.current-exe --no-config-file --commands $"(staged)\ne2e_ll" | str trim)"

_starbase_deactivate

print $"foo=($env | get --optional E2E_FOO | default REMOVED)"
print $"path=($env.{path_key})"
print $"hooks=(registered)"
print $"staged=(staged_count) (staged_defs)"

source "./hook.nu"

_starbase_activate

print $"foo=($env | get --optional E2E_FOO | default MISSING)"
print $"hooks=(registered)"
print $"staged=(staged_count) (staged_defs)"
"#
        ),
    );

    if let Some(output) = run_script(&sandbox, "nu", &["./test.nu"]) {
        assert_eq!(
            stdout(&output),
            "foo=123\ngone=REMOVED\npath=/e2e-stub-path\nhooks=1,1\n\
             staged=1 alias e2e_ll = echo ALIASOK,hide e2e_gone\nparsed=ALIASOK\n\
             foo=REMOVED\npath=/e2e-restored-path\nhooks=0,0\nstaged=1 hide e2e_ll\n\
             foo=123\nhooks=1,1\nstaged=1 alias e2e_ll = echo ALIASOK,hide e2e_gone\n"
        );
    }
}

// The murex `onPrompt` event only fires interactively, so this drives the
// hook functions directly. Reading an unset variable is a hard error in
// murex, so the removal is asserted through a child process instead.
#[cfg(unix)]
#[test]
fn murex_activates_and_deactivates() {
    let hook = format_hook(
        ShellType::Murex,
        r#"out "export E2E_FOO=123""#,
        r#"out "unset E2E_FOO""#,
    );

    let sandbox = create_empty_sandbox();
    sandbox.create_file("hook.mx", &hook);
    sandbox.create_file(
        "test.mx",
        r#"
source ./hook.mx
_starbase_activate
out $ENV.E2E_FOO
_starbase_deactivate
sh -c 'echo "${E2E_FOO:-removed}"'
source ./hook.mx
_starbase_activate
out $ENV.E2E_FOO
out done
"#,
    );

    if let Some(output) = run_script(&sandbox, "murex", &["./test.mx"]) {
        assert_eq!(stdout(&output), "123\nremoved\n123\ndone\n");
    }
}

// POSIX shells have no native hook, so the rendered hook shadows the `cd`
// builtin. The activate stub increments a counter so the assertions prove
// the hook fires exactly once per cd (re-sourcing must not stack wrappers).
// Sh, Dash, and Ash all render the same hook, so one script drives all
// three binaries.
#[cfg(unix)]
#[test]
fn posix_shells_activate_and_deactivate() {
    let hook = format_hook(
        ShellType::Sh,
        r#"printf 'export E2E_FOO="123";\nexport E2E_COUNT=$((${E2E_COUNT:-0}+1));\n'"#,
        r#"printf 'unset E2E_FOO;\nunset E2E_COUNT;\n'"#,
    );

    let sandbox = create_empty_sandbox();
    sandbox.create_file("hook.sh", &hook);
    sandbox.create_file(
        "test.sh",
        r#"
root="$(pwd)"
. "$root/hook.sh"
cd /
echo "FOO=$E2E_FOO COUNT=$E2E_COUNT"
. "$root/hook.sh"
cd /tmp
echo "COUNT=$E2E_COUNT"
_starbase_deactivate
echo "FOO=${E2E_FOO:-unset}"
cd /usr
echo "after COUNT=${E2E_COUNT:-0}"
. "$root/hook.sh"
cd /
echo "cycle FOO=$E2E_FOO COUNT=$E2E_COUNT"
"#,
    );

    for bin in ["sh", "dash", "ash"] {
        if let Some(output) = run_script(&sandbox, bin, &["./test.sh"]) {
            assert_eq!(
                stdout(&output),
                "FOO=123 COUNT=1\nCOUNT=2\nFOO=unset\nafter COUNT=0\ncycle FOO=123 COUNT=1\n",
                "shell: {bin}"
            );
        }
    }
}

// Xonsh statements are applied with `execx`, and handlers on the set-like
// `events.on_chdir` are matched by name. Repeating the hook inline is fine
// (Python rebinds the defs), so the second copy exercises the dedup guard
// and the third re-activates after deactivation. The deactivation statements
// are generated by the crate, and include an env var and an alias that were
// never set, to prove a missing key doesn't abort the rest of the block.
#[cfg(unix)]
#[test]
fn xonsh_activates_and_deactivates() {
    let shell = ShellType::Xonsh.build();
    let sandbox = create_empty_sandbox();

    sandbox.create_file(
        "activate.xsh",
        format!(
            "{}\n{}\n{}\n",
            shell.format_env_set("E2E_FOO", "123"),
            shell.format_env_set("E2E_BAR", "456"),
            shell.format_alias_set("e2e_alias", "echo aliased"),
        ),
    );
    sandbox.create_file(
        "deactivate.xsh",
        format!(
            "{}\n{}\n{}\n{}\n{}\n",
            shell.format_env_unset("E2E_FOO"),
            shell.format_env_unset("E2E_NEVER_SET"),
            shell.format_env_unset("E2E_BAR"),
            shell.format_alias_unset("e2e_never_set"),
            shell.format_alias_unset("e2e_alias"),
        ),
    );

    let hook = format_hook(
        ShellType::Xonsh,
        &format!("cat {}/activate.xsh", sandbox.path().display()),
        &format!("cat {}/deactivate.xsh", sandbox.path().display()),
    );

    sandbox.create_file(
        "test.xsh",
        format!(
            "{hook}\n{hook}\n{}\n{hook}\n{}",
            r#"
def _counts():
    return "%d,%d" % (
        sum(1 for h in events.on_chdir if getattr(h, '__name__', '') == '_starbase_activate'),
        sum(1 for h in events.on_pre_prompt if getattr(h, '__name__', '') == '_starbase_activate'),
    )

_starbase_activate()
print("foo=" + ${...}.get('E2E_FOO', 'unset'))
print("alias=" + str('e2e_alias' in aliases))
print("hooks=" + _counts())
$E2E_FOO = 'reset'
cd /
print("chdir foo=" + ${...}.get('E2E_FOO', 'unset'))
_starbase_deactivate()
print("hooks=" + _counts())
print("removed foo=" + ${...}.get('E2E_FOO', 'unset'))
print("removed alias=" + str('e2e_alias' in aliases))
print("funcs=" + str('_starbase_activate' in globals() or '_starbase_deactivate' in globals()))
cd /tmp
print("after foo=" + ${...}.get('E2E_FOO', 'unset'))
"#,
            r#"
cd /
print("cycle foo=" + ${...}.get('E2E_FOO', 'unset'))
"#
        ),
    );

    if let Some(output) = run_script(&sandbox, "xonsh", &["--no-rc", "./test.xsh"]) {
        assert_eq!(
            stdout(&output),
            "foo=123\nalias=True\nhooks=1,1\nchdir foo=123\nhooks=0,0\nremoved foo=unset\nremoved alias=False\nfuncs=False\nafter foo=unset\ncycle foo=123\n"
        );
    }
}

// Xonsh consumers source the hook with `execx($(<command>))`, which does not
// evaluate into the shell's namespace, so the hook exports both functions
// itself. Without that, the deactivate function raises a `NameError`.
#[cfg(unix)]
#[test]
fn xonsh_hook_functions_are_callable_after_execx() {
    let sandbox = create_empty_sandbox();
    let hook = format_hook(
        ShellType::Xonsh,
        r#"printf '$E2E_FOO = f"123"\n'"#,
        r#"printf '${...}.pop("E2E_FOO", None)\n'"#,
    );

    sandbox.create_file("hook.xsh", &hook);
    sandbox.create_file(
        "test.xsh",
        format!(
            r#"execx($(cat {}/hook.xsh))
print("callable=" + str('_starbase_activate' in globals() and '_starbase_deactivate' in globals()))
_starbase_activate()
print("foo=" + ${{...}}.get('E2E_FOO', 'unset'))
_starbase_deactivate()
print("foo=" + ${{...}}.get('E2E_FOO', 'unset'))
print("callable=" + str('_starbase_activate' in globals() or '_starbase_deactivate' in globals()))
"#,
            sandbox.path().display()
        ),
    );

    if let Some(output) = run_script(&sandbox, "xonsh", &["--no-rc", "./test.xsh"]) {
        assert_eq!(
            stdout(&output),
            "callable=True\nfoo=123\nfoo=unset\ncallable=False\n"
        );
    }
}

// Windows PowerShell 5.1 hooks by wrapping the global `prompt` function.
// Prompts never render under `-File`, so the trigger is simulated by
// invoking `prompt` directly. The activation exports run a native command
// (exit 3) to prove `$LASTEXITCODE` is restored.
#[test]
fn powershell_activates_and_deactivates() {
    let hook = format_hook(
        ShellType::PowerShell,
        r#"Write-Output '& cmd /c exit 3; $env:E2E_FOO = "123"; $env:E2E_BAR = "456";'"#,
        r#"Write-Output 'Remove-Item -LiteralPath "env:E2E_FOO" -ErrorAction Ignore; Remove-Item -LiteralPath "env:E2E_BAR" -ErrorAction Ignore;'"#,
    );

    let sandbox = create_empty_sandbox();
    sandbox.create_file("hook.ps1", &hook);
    sandbox.create_file(
        "test.ps1",
        r#"
. $PSScriptRoot/hook.ps1
. $PSScriptRoot/hook.ps1
Write-Output "wrapped=$($function:prompt.ToString().Contains('_starbase_activate'))"
Write-Output "nested=$($global:_starbase_activate_prompt.ToString().Contains('_starbase_activate'))"
& cmd /c exit 7
prompt > $null
Write-Output "E2E_FOO=$env:E2E_FOO E2E_BAR=$env:E2E_BAR"
Write-Output "EXIT=$global:LASTEXITCODE"
_starbase_deactivate
Write-Output "unwrapped=$(-not $function:prompt.ToString().Contains('_starbase_activate'))"
Write-Output "E2E_FOO=$(if ($env:E2E_FOO) { $env:E2E_FOO } else { 'unset' })"
prompt > $null
Write-Output "still=$(if ($env:E2E_FOO) { $env:E2E_FOO } else { 'unset' })"
Write-Output "FUNCS=$((Get-Command _starbase_activate, _starbase_deactivate -ErrorAction Ignore | Measure-Object).Count)"
. $PSScriptRoot/hook.ps1
prompt > $null
Write-Output "cycle=$env:E2E_FOO"
"#,
    );

    if let Some(output) = run_script(
        &sandbox,
        "powershell",
        &[
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            "./test.ps1",
        ],
    ) {
        assert_eq!(
            stdout(&output),
            if cfg!(target_os = "macos") {
                "wrapped=True\nnested=False\nE2E_FOO=123 E2E_BAR=456\nEXIT=\nunwrapped=True\nE2E_FOO=unset\nstill=unset\nFUNCS=0\ncycle=123\n"
            } else {
                "wrapped=True\nnested=False\nE2E_FOO=123 E2E_BAR=456\nEXIT=7\nunwrapped=True\nE2E_FOO=unset\nstill=unset\nFUNCS=0\ncycle=123\n"
            }
        );
    }
}

#[test]
fn pwsh_activates_and_deactivates() {
    // The activation exports run a native command (exit 3) to prove the hook
    // restores the `$LASTEXITCODE` that was live before it fired.
    let hook = format_hook(
        ShellType::Pwsh,
        r#"Write-Output '& pwsh -NoProfile -Command ''exit 3''; $env:E2E_FOO = "123"; $env:E2E_BAR = "456";'"#,
        r#"Write-Output 'Remove-Item -LiteralPath "env:E2E_FOO" -ErrorAction Ignore; Remove-Item -LiteralPath "env:E2E_BAR" -ErrorAction Ignore;'"#,
    );

    let sandbox = create_empty_sandbox();
    sandbox.create_file("hook.ps1", &hook);
    sandbox.create_file(
        "test.ps1",
        r#"
. $PSScriptRoot/hook.ps1
. $PSScriptRoot/hook.ps1
Write-Output "wrapped=$($function:prompt.ToString().Contains('_starbase_activate'))"
& pwsh -NoProfile -Command 'exit 7'
Set-Location ([System.IO.Path]::GetTempPath())
Write-Output "E2E_FOO=$env:E2E_FOO E2E_BAR=$env:E2E_BAR"
Write-Output "EXIT=$global:LASTEXITCODE"
Write-Output "HANDLERS=$($ExecutionContext.SessionState.InvokeCommand.LocationChangedAction.GetInvocationList().Count)"
_starbase_deactivate
$action = $ExecutionContext.SessionState.InvokeCommand.LocationChangedAction
if ($null -eq $action) { Write-Output "HANDLERS=0" } else { Write-Output "HANDLERS=$($action.GetInvocationList().Count)" }
Write-Output "unwrapped=$(-not $function:prompt.ToString().Contains('_starbase_activate'))"
Set-Location /
Write-Output "E2E_FOO=$(if ($env:E2E_FOO) { $env:E2E_FOO } else { 'unset' })"
prompt > $null
Write-Output "still=$(if ($env:E2E_FOO) { $env:E2E_FOO } else { 'unset' })"
Write-Output "FUNCS=$((Get-Command _starbase_activate, _starbase_deactivate -ErrorAction Ignore | Measure-Object).Count)"
. $PSScriptRoot/hook.ps1
Set-Location ([System.IO.Path]::GetTempPath())
Write-Output "E2E_FOO=$env:E2E_FOO"
Write-Output "HANDLERS=$($ExecutionContext.SessionState.InvokeCommand.LocationChangedAction.GetInvocationList().Count)"
Remove-Item -LiteralPath 'env:E2E_FOO' -ErrorAction Ignore
prompt > $null
Write-Output "prompt=$env:E2E_FOO"
"#,
    );

    if let Some(output) = run_script(
        &sandbox,
        "pwsh",
        &[
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            "./test.ps1",
        ],
    ) {
        assert_eq!(
            stdout(&output),
            "wrapped=True\nE2E_FOO=123 E2E_BAR=456\nEXIT=7\nHANDLERS=1\nHANDLERS=0\n\
             unwrapped=True\nE2E_FOO=unset\nstill=unset\nFUNCS=0\n\
             E2E_FOO=123\nHANDLERS=1\nprompt=123\n"
        );
    }
}
