// End-to-end tests for the activation flow. Each test renders the three hooks
// a session needs — the activate function, the deactivate function, and the
// registration that calls the activate one — runs the result through the real
// shell binary, and asserts that:
//
// - the emitted activation statements are evaluated and environment changes
//   propagate to the session,
// - the change-dir trigger fires,
// - sourcing the hook twice registers it once,
// - the deactivate function reverts the environment, and leaves the
//   registration and both functions in place, so a later trigger activates
//   again.
//
// These run non-interactively, where prompt triggers never fire, so a shell
// that registers on both is only exercised through its change-dir trigger —
// the prompt registration is asserted by counting handlers instead.
//
//
// A test is skipped when its shell is not installed, unless the shell is
// listed in the `STARBASE_REQUIRED_SHELLS` environment variable
// (comma-separated binary names), which CI sets per operating system so a
// broken install step fails loudly instead of silently skipping.

use starbase_sandbox::{Sandbox, create_empty_sandbox};
use starbase_shell::{Hook, ShellType, Statement};
use std::io;
use std::process::{Command, Output};

/// Render the three hooks a session needs, the way a consumer emits them:
/// both functions, then the registration that calls the activate one.
fn format_hook(shell: ShellType, activate_command: &str, deactivate_command: &str) -> String {
    let shell = shell.build();

    [
        shell
            .format_hook(Hook::Activate {
                command: activate_command.into(),
                function: "_starbase_activate".into(),
            })
            .unwrap(),
        shell
            .format_hook(Hook::Deactivate {
                command: deactivate_command.into(),
                function: "_starbase_deactivate".into(),
            })
            .unwrap(),
        shell
            .format_hook(Hook::RegisterHandlers {
                function: "_starbase_activate".into(),
            })
            .unwrap(),
    ]
    .join("\n\n")
}

/// Render the hook that removes what the registration installed.
fn format_unhook(shell: ShellType) -> String {
    shell
        .build()
        .format_hook(Hook::UnregisterHandlers {
            function: "_starbase_activate".into(),
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
    sandbox.create_file("unhook.sh", format_unhook(ShellType::Bash));

    // String form `PROMPT_COMMAND`: invoking the hook (as the prompt would)
    // applies the exports, sourcing twice registers once, and deactivating
    // reverts the environment while leaving the registration alone.
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
type -t _starbase_activate >/dev/null 2>&1 && printf 'functions kept\n'
_starbase_activate
printf '%s\n' "$PROMPT_COMMAND"
printf 'E2E_FOO=%s\n' "$E2E_FOO"
source ./unhook.sh
printf 'unhooked %s\n' "${PROMPT_COMMAND:-empty}"
"#,
    );

    if let Some(output) = run_script(&sandbox, "bash", &["./string.sh"]) {
        assert_eq!(
            stdout(&output),
            "_starbase_activate;starship_precmd\nE2E_FOO=123 E2E_BAR=456\n_starbase_activate;starship_precmd\nE2E_FOO=unset E2E_BAR=unset\nfunctions kept\n_starbase_activate;starship_precmd\nE2E_FOO=123\nunhooked starship_precmd\n"
        );
    }

    // Array form `PROMPT_COMMAND` (bash >= 5.1) with multiple entries:
    // registration adds one entry and leaves the rest alone, and repeating it
    // does not add a second.
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
source ./unhook.sh
printf '%s\n' "${PROMPT_COMMAND[*]}"
"#,
    );

    if let Some(output) = run_script(&sandbox, "bash", &["./array.sh"]) {
        assert_eq!(
            stdout(&output),
            "_starbase_activate starship_precmd other_thing\n_starbase_activate starship_precmd other_thing\n_starbase_activate starship_precmd other_thing\nstarship_precmd other_thing\n"
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
    sandbox.create_file("unhook.zsh", format_unhook(ShellType::Zsh));
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
whence -w _starbase_activate >/dev/null 2>&1 && print -r -- "functions kept"
cd /tmp
print -r -- "after cd E2E_FOO=${E2E_FOO:-unset}"
source "$root/hook.zsh"
cd /usr
print -r -- "HOOKS=${#chpwd_functions[@]},${#precmd_functions[@]} E2E_FOO=${E2E_FOO:-unset}"
source "$root/unhook.zsh"
export E2E_FOO=changed
cd /
print -r -- "unhooked HOOKS=${#chpwd_functions[@]},${#precmd_functions[@]} E2E_FOO=$E2E_FOO"
"#,
    );

    // -f skips rc files so user config cannot interfere
    if let Some(output) = run_script(&sandbox, "zsh", &["-f", "./test.zsh"]) {
        assert_eq!(
            stdout(&output),
            "E2E_FOO=123 E2E_BAR=456\nHOOKS=1,1\nHOOKS=1,1\nE2E_FOO=unset E2E_BAR=unset\nfunctions kept\nafter cd E2E_FOO=123\nHOOKS=1,1 E2E_FOO=123\nunhooked HOOKS=0,0 E2E_FOO=changed\n"
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
    sandbox.create_file("unhook.fish", format_unhook(ShellType::Fish));
    sandbox.create_file(
        "test.fish",
        r#"
function handlers
  printf 'handlers=%s,%s\n' \
    (functions --handlers-type variable | string match -r '^PWD _starbase_activate_on_context$' | count) \
    (functions --handlers-type generic | string match -r '^fish_prompt _starbase_activate_on_context$' | count)
end

set root (pwd)
source $root/hook.fish
cd /
printf 'E2E_FOO=%s E2E_BAR=%s\n' $E2E_FOO $E2E_BAR
handlers
_starbase_deactivate
handlers
if set -q E2E_FOO
  printf 'still set\n'
else
  printf 'E2E_FOO unset\n'
end
functions -q _starbase_activate; and printf 'functions kept\n'
cd /tmp
printf 'E2E_FOO=%s\n' $E2E_FOO
source $root/unhook.fish
handlers
"#,
    );

    if let Some(output) = run_script(&sandbox, "fish", &["--no-config", "./test.fish"]) {
        assert_eq!(
            stdout(&output),
            "E2E_FOO=123 E2E_BAR=456\nhandlers=1,1\nhandlers=1,1\nE2E_FOO unset\nfunctions kept\nE2E_FOO=123\nhandlers=0,0\n"
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
    sandbox.create_file("unhook.elv", format_unhook(ShellType::Elvish));

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
eval (slurp < $root/unhook.elv)
set-env E2E_FOO changed
cd /
echo unhooked=$E:E2E_FOO hooks=(count $after-chdir)
"#
        ),
    );

    if let Some(output) = run_script(&sandbox, "elvish", &["./test.elv"]) {
        assert_eq!(
            stdout(&output),
            "direct=123\nchdir=123,456\nhooks=1\nhooks=1\nremoved=\nafter=123\ncycle=123 hooks=1\nunhooked=changed hooks=0\n"
        );
    }
}

// Nushell has no runtime `eval`, so the hook stages the statements in a file
// and applies them with a second hook entry that `source`s it. Hooks only fire
// inside the interactive REPL, so this drives the functions directly (as the
// init call does) and asserts what was staged, that feeding it to a fresh nu
// really does apply it, and the registration list.
#[test]
fn nu_activates_and_deactivates() {
    let path_key = if cfg!(windows) { "Path" } else { "PATH" };

    let hook = format_hook(
        ShellType::Nu,
        &format!(
            r#"echo "$env.E2E_FOO = '123'\nhide-env --ignore-errors E2E_GONE\n$env.{path_key} = ([] | prepend \"/e2e-stub-path\" | uniq)\nalias e2e_ll = print ALIASOK""#
        ),
        r#"echo "hide-env --ignore-errors E2E_FOO\nhide e2e_ll""#,
    );

    let sandbox = create_empty_sandbox();
    sandbox.create_file("hook.nu", &hook);
    sandbox.create_file("unhook.nu", format_unhook(ShellType::Nu));

    // Sourcing the same file twice dedupes the `export def`s at parse time, but
    // re-runs `export-env`, so the second source exercises the registration
    // dedup guard. Each function writes its statements to a file of its own and
    // stages a one shot entry to apply them, so calling one adds an entry to
    // `pre_prompt` rather than changing what is registered.
    sandbox.create_file(
        "test.nu",
        format!(
            r#"const sb_activate = $"($nu.temp-dir)/_starbase_activate-($nu.pid).nu"
const sb_deactivate = $"($nu.temp-dir)/_starbase_deactivate-($nu.pid).nu"

def registered [] {{
    let pwd_list = ($env.config | get --optional hooks.env_change.PWD) | default []
    let prompt_list = ($env.config | get --optional hooks.pre_prompt) | default []

    $"($pwd_list | length),($prompt_list | length)"
}}

def statements [file] {{
    open $file | str trim | lines | str join ","
}}

source "./hook.nu"
source "./hook.nu"

print $"hooks=(registered)"

_starbase_activate

print $"staged=(statements $sb_activate) hooks=(registered)"

# Hooks never fire in a script, so the staged statements are fed to a fresh nu
# to prove they apply: the alias is defined, the variable is set, the preset one
# is removed, and the path is replaced.
let probe = ([
    "$env.E2E_GONE = 'preset'"
    (open $sb_activate)
    "e2e_ll"
    "print $env.E2E_FOO"
    "print ('E2E_GONE' in $env)"
    "print ($env.{path_key} | str join ',')"
] | str join (char newline))

print $"applied=(^$nu.current-exe --no-config-file --commands $probe | str trim | split row (char newline) | str join ',')"

_starbase_deactivate

# Deactivating stages the reversal the same way, and leaves the registration
# alone, so the writer keeps running on every trigger
print $"teardown=(statements $sb_deactivate) hooks=(registered)"

source "./unhook.nu"

print $"unhooked=(registered)"
"#
        ),
    );

    if let Some(output) = run_script(&sandbox, "nu", &["./test.nu"]) {
        assert_eq!(
            stdout(&output),
            "hooks=1,1\n\
             staged=$env.E2E_FOO = '123',hide-env --ignore-errors E2E_GONE,\
             $env.PATH_KEY = ([] | prepend \"/e2e-stub-path\" | uniq),\
             alias e2e_ll = print ALIASOK hooks=1,2\n\
             applied=ALIASOK,123,false,/e2e-stub-path\n\
             teardown=hide-env --ignore-errors E2E_FOO,hide e2e_ll hooks=1,3\n\
             unhooked=0,2\n"
                .replace("PATH_KEY", path_key)
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
    sandbox.create_file("unhook.mx", format_unhook(ShellType::Murex));
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
out registered=${runtime --events -> [onPrompt] -> len}
source ./unhook.mx
out unhooked=${runtime --events -> [onPrompt] -> len}
out done
"#,
    );

    if let Some(output) = run_script(&sandbox, "murex", &["./test.mx"]) {
        assert_eq!(
            stdout(&output),
            "123\nremoved\n123\nregistered=1\nunhooked=0\ndone\n"
        );
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
    sandbox.create_file("unhook.sh", format_unhook(ShellType::Sh));
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
echo "after FOO=${E2E_FOO:-unset} COUNT=${E2E_COUNT:-0}"
. "$root/unhook.sh"
cd /
echo "unhooked FOO=${E2E_FOO:-unset} COUNT=${E2E_COUNT:-0}"
"#,
    );

    for bin in ["sh", "dash", "ash"] {
        if let Some(output) = run_script(&sandbox, bin, &["./test.sh"]) {
            assert_eq!(
                stdout(&output),
                "FOO=123 COUNT=1\nCOUNT=2\nFOO=unset\nafter FOO=123 COUNT=1\nunhooked FOO=123 COUNT=1\n",
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
            shell.format(Statement::SetAlias {
                name: "e2e_alias",
                value: "echo aliased",
                hook: true,
            }),
        ),
    );
    sandbox.create_file(
        "deactivate.xsh",
        format!(
            "{}\n{}\n{}\n{}\n{}\n",
            shell.format_env_unset("E2E_FOO"),
            shell.format_env_unset("E2E_NEVER_SET"),
            shell.format_env_unset("E2E_BAR"),
            shell.format(Statement::UnsetAlias {
                name: "e2e_never_set",
                hook: true,
            }),
            shell.format(Statement::UnsetAlias {
                name: "e2e_alias",
                hook: true,
            }),
        ),
    );

    let hook = format_hook(
        ShellType::Xonsh,
        &format!("cat {}/activate.xsh", sandbox.path().display()),
        &format!("cat {}/deactivate.xsh", sandbox.path().display()),
    );

    let teardown = format!(
        r#"
cd /
print("cycle foo=" + ${{...}}.get('E2E_FOO', 'unset'))

{unhook}

print("unhooked=" + _counts())
$E2E_FOO = 'changed'
cd /tmp
print("unhooked foo=" + ${{...}}.get('E2E_FOO', 'unset'))
"#,
        unhook = format_unhook(ShellType::Xonsh)
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
print("funcs=" + str('_starbase_activate' in globals() and '_starbase_deactivate' in globals()))
cd /tmp
print("after foo=" + ${...}.get('E2E_FOO', 'unset'))
"#,
            teardown
        ),
    );

    if let Some(output) = run_script(&sandbox, "xonsh", &["--no-rc", "./test.xsh"]) {
        assert_eq!(
            stdout(&output),
            "foo=123\nalias=True\nhooks=1,1\nchdir foo=123\nhooks=1,1\nremoved foo=unset\nremoved alias=False\nfuncs=True\nafter foo=123\ncycle foo=123\nunhooked=0,0\nunhooked foo=changed\n"
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
print("callable=" + str('_starbase_activate' in globals() and '_starbase_deactivate' in globals()))
"#,
            sandbox.path().display()
        ),
    );

    if let Some(output) = run_script(&sandbox, "xonsh", &["--no-rc", "./test.xsh"]) {
        assert_eq!(
            stdout(&output),
            "callable=True\nfoo=123\nfoo=unset\ncallable=True\n"
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
    sandbox.create_file("unhook.ps1", format_unhook(ShellType::PowerShell));
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
Write-Output "wrapped=$($function:prompt.ToString().Contains('_starbase_activate'))"
Write-Output "E2E_FOO=$(if ($env:E2E_FOO) { $env:E2E_FOO } else { 'unset' })"
Write-Output "FUNCS=$((Get-Command _starbase_activate, _starbase_deactivate -ErrorAction Ignore | Measure-Object).Count)"
prompt > $null
Write-Output "again=$env:E2E_FOO"
. $PSScriptRoot/unhook.ps1
Write-Output "unhooked=$($function:prompt.ToString().Contains('_starbase_activate'))"
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
                "wrapped=True\nnested=False\nE2E_FOO=123 E2E_BAR=456\nEXIT=\nwrapped=True\nE2E_FOO=unset\nFUNCS=2\nagain=123\nunhooked=False\n"
            } else {
                "wrapped=True\nnested=False\nE2E_FOO=123 E2E_BAR=456\nEXIT=7\nwrapped=True\nE2E_FOO=unset\nFUNCS=2\nagain=123\nunhooked=False\n"
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
    sandbox.create_file("unhook.ps1", format_unhook(ShellType::Pwsh));
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
Write-Output "HANDLERS=$($ExecutionContext.SessionState.InvokeCommand.LocationChangedAction.GetInvocationList().Count)"
Write-Output "wrapped=$($function:prompt.ToString().Contains('_starbase_activate'))"
Write-Output "E2E_FOO=$(if ($env:E2E_FOO) { $env:E2E_FOO } else { 'unset' })"
Write-Output "FUNCS=$((Get-Command _starbase_activate, _starbase_deactivate -ErrorAction Ignore | Measure-Object).Count)"
Set-Location /
Write-Output "again=$env:E2E_FOO"
Remove-Item -LiteralPath 'env:E2E_FOO' -ErrorAction Ignore
prompt > $null
Write-Output "prompt=$env:E2E_FOO"
. $PSScriptRoot/unhook.ps1
$action = $ExecutionContext.SessionState.InvokeCommand.LocationChangedAction
if ($null -eq $action) { Write-Output "HANDLERS=0" } else { Write-Output "HANDLERS=$($action.GetInvocationList().Count)" }
Write-Output "unhooked=$($function:prompt.ToString().Contains('_starbase_activate'))"
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
            "wrapped=True\nE2E_FOO=123 E2E_BAR=456\nEXIT=7\nHANDLERS=1\nHANDLERS=1\n\
             wrapped=True\nE2E_FOO=unset\nFUNCS=2\n\
             again=123\nprompt=123\nHANDLERS=0\nunhooked=False\n"
        );
    }
}
