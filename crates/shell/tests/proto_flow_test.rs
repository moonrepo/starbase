// End-to-end test of the full activate → deactivate lifecycle, composed the
// way proto composes it:
//
// - `proto activate <shell>` prints Activate + Deactivate + RegisterHandlers,
//   which the profile evaluates as one block. The registered handler calls the
//   activate function.
// - The activate command prints statements that set the environment.
// - The deactivate command prints statements that unset the environment,
//   followed by the UnregisterHandlers hook and an UnsetFunction for both
//   functions.
//
// The last point is what the other suites do not cover: the teardown code is
// evaluated *inside* the deactivate function, from the command's output — not
// at the top level of the session. Shells where evaluation context matters
// (elvish's `eval` namespace, xonsh's `execx`, nu's staged `source`) can
// behave differently there.
//
// After deactivation the session must be inert: the environment stays torn
// down across a directory change (handlers unregistered), and calling the
// activate function does nothing (functions removed).

#![cfg(unix)]

mod pty;

use pty::Pty;
use starbase_sandbox::create_empty_sandbox;
use starbase_shell::{Hook, ShellType, Statement};

struct Case {
    shell: ShellType,
    bin: &'static str,
    args: &'static [&'static str],
    extension: &'static str,
    evaluate: fn(&str) -> String,
    deactivate_call: &'static str,
    activates_on_prompt: bool,
    term: &'static str,
}

/// Return the value from the last `<label> foo=<value>` line.
fn reported(output: &str, label: &str) -> String {
    let prefix = format!("{label} foo=");

    output
        .lines()
        .filter_map(|line| {
            line.find(&prefix)
                .map(|index| &line[index + prefix.len()..])
        })
        .next_back()
        .unwrap_or_else(|| panic!("no `{prefix}` line in transcript:\n{output}"))
        .trim()
        .to_owned()
}

fn run_case(case: Case) {
    let shell = case.shell.build();
    let sandbox = create_empty_sandbox();
    let root = sandbox.path().display().to_string();

    sandbox.create_file(
        "report.sh",
        "printf '%s foo=%s\\n%s ok\\n' \"$1\" \"${E2E_FOO:-unset}\" \"$1\"\n",
    );

    // What `proto activate --export` would print
    sandbox.create_file(
        "activate.txt",
        format!("{}\n", shell.format_env_set("E2E_FOO", "123")),
    );

    // What `proto deactivate --export` would print: reverse the environment,
    // unregister the handlers, remove both functions
    sandbox.create_file(
        "deactivate.txt",
        format!(
            "{}\n{}\n{}\n{}\n",
            shell.format_env_unset("E2E_FOO"),
            shell
                .format_hook(Hook::UnregisterHandlers {
                    function: "proto_activate".into(),
                })
                .unwrap(),
            shell.format(Statement::UnsetFunction {
                name: "proto_activate",
                hook: true,
            }),
            shell.format(Statement::UnsetFunction {
                name: "proto_deactivate",
                hook: true,
            }),
        ),
    );

    // What `proto activate` prints for the profile to evaluate
    sandbox.create_file(
        format!("hook.{}", case.extension),
        [
            shell
                .format_hook(Hook::Activate {
                    command: format!("cat {root}/activate.txt"),
                    function: "proto_activate".into(),
                })
                .unwrap(),
            shell
                .format_hook(Hook::Deactivate {
                    command: format!("cat {root}/deactivate.txt"),
                    function: "proto_deactivate".into(),
                })
                .unwrap(),
            shell
                .format_hook(Hook::RegisterHandlers {
                    function: "proto_activate".into(),
                })
                .unwrap(),
        ]
        .join("\n\n"),
    );

    let Some(mut session) = Pty::spawn_with_term(case.bin, case.args, case.term) else {
        return;
    };

    let report = |label: &str| format!("sh {root}/report.sh {label}");

    session.wait_until_ready(&report("READY"), "READY ok");
    session.send(&(case.evaluate)(&format!("{root}/hook.{}", case.extension)));

    // Activation: by prompt where the shell has one, by cd everywhere
    let output = session.sync(&report("S1"), "S1 ok");
    let expected = if case.activates_on_prompt {
        "123"
    } else {
        "unset"
    };

    assert_eq!(reported(&output, "S1"), expected, "{}", case.shell);

    let output = session.sync(&format!("cd /tmp ; {}", report("S2")), "S2 ok");

    assert_eq!(reported(&output, "S2"), "123", "{}", case.shell);

    // Teardown: one call must revert the environment, unregister the handlers
    // and remove the functions, all through the command's printed statements
    session.send(case.deactivate_call);

    let output = session.sync(&report("S3"), "S3 ok");

    assert_eq!(reported(&output, "S3"), "unset", "{}", case.shell);

    // Handlers are gone, so a directory change re-activates nothing
    let output = session.sync(&format!("cd / ; {}", report("S4")), "S4 ok");

    assert_eq!(reported(&output, "S4"), "unset", "{}", case.shell);

    // The functions are gone too, so calling the activate function is an
    // error rather than an activation
    session.send(
        case.deactivate_call
            .replace("deactivate", "activate")
            .as_str(),
    );

    let output = session.sync(&report("S5"), "S5 ok");

    assert_eq!(reported(&output, "S5"), "unset", "{}", case.shell);

    drop(sandbox);
}

#[test]
fn bash_full_lifecycle() {
    run_case(Case {
        shell: ShellType::Bash,
        term: "dumb",
        bin: "bash",
        args: &["--norc", "-i"],
        extension: "sh",
        evaluate: |path| format!("source {path}"),
        deactivate_call: "proto_deactivate",
        activates_on_prompt: true,
    });
}

#[test]
fn zsh_full_lifecycle() {
    run_case(Case {
        shell: ShellType::Zsh,
        term: "dumb",
        bin: "zsh",
        args: &["-f", "-i"],
        extension: "zsh",
        evaluate: |path| format!("source {path}"),
        deactivate_call: "proto_deactivate",
        activates_on_prompt: true,
    });
}

#[test]
fn fish_full_lifecycle() {
    run_case(Case {
        shell: ShellType::Fish,
        term: "dumb",
        bin: "fish",
        args: &["--no-config", "-i"],
        extension: "fish",
        evaluate: |path| format!("source {path}"),
        deactivate_call: "proto_deactivate",
        activates_on_prompt: true,
    });
}

#[test]
fn elvish_full_lifecycle() {
    run_case(Case {
        shell: ShellType::Elvish,
        term: "dumb",
        bin: "elvish",
        args: &["-norc"],
        extension: "elv",
        evaluate: |path| format!("eval (slurp < {path})"),
        deactivate_call: "proto_deactivate",
        activates_on_prompt: true,
    });
}

#[test]
fn xonsh_full_lifecycle() {
    run_case(Case {
        shell: ShellType::Xonsh,
        term: "xterm-256color",
        bin: "xonsh",
        args: &["--no-rc", "-i"],
        extension: "xsh",
        evaluate: |path| format!("execx($(cat {path}))"),
        deactivate_call: "proto_deactivate()",
        activates_on_prompt: true,
    });
}

#[test]
fn nu_full_lifecycle() {
    run_case(Case {
        shell: ShellType::Nu,
        term: "dumb",
        bin: "nu",
        args: &["--no-config-file"],
        extension: "nu",
        evaluate: |path| format!("source \"{path}\""),
        deactivate_call: "proto_deactivate",
        activates_on_prompt: true,
    });
}

#[test]
fn sh_full_lifecycle() {
    run_case(Case {
        shell: ShellType::Sh,
        term: "dumb",
        bin: "sh",
        args: &["-i"],
        extension: "sh",
        evaluate: |path| format!(". {path}"),
        deactivate_call: "proto_deactivate",
        activates_on_prompt: false,
    });
}

#[test]
fn dash_full_lifecycle() {
    run_case(Case {
        shell: ShellType::Dash,
        term: "dumb",
        bin: "dash",
        args: &["-i"],
        extension: "sh",
        evaluate: |path| format!(". {path}"),
        deactivate_call: "proto_deactivate",
        activates_on_prompt: false,
    });
}

// Murex's line editor cannot be driven through a pty, so its lifecycle runs
// as a script: the prompt trigger never fires there, but the composed
// teardown — unregister and function removal through the deactivate command's
// output — is exactly what the other shells needed a pty to prove.
#[test]
fn murex_full_lifecycle() {
    use std::process::Command;

    let shell = ShellType::Murex.build();
    let sandbox = create_empty_sandbox();
    let root = sandbox.path().display().to_string();

    sandbox.create_file(
        "activate.txt",
        format!("{}\n", shell.format_env_set("E2E_FOO", "123")),
    );
    sandbox.create_file(
        "deactivate.txt",
        format!(
            "{}\n{}\n{}\n{}\n",
            shell.format_env_unset("E2E_FOO"),
            shell
                .format_hook(Hook::UnregisterHandlers {
                    function: "proto_activate".into(),
                })
                .unwrap(),
            shell.format(Statement::UnsetFunction {
                name: "proto_activate",
                hook: true,
            }),
            shell.format(Statement::UnsetFunction {
                name: "proto_deactivate",
                hook: true,
            }),
        ),
    );
    sandbox.create_file(
        "hook.mx",
        [
            shell
                .format_hook(Hook::Activate {
                    command: format!("cat {root}/activate.txt"),
                    function: "proto_activate".into(),
                })
                .unwrap(),
            shell
                .format_hook(Hook::Deactivate {
                    command: format!("cat {root}/deactivate.txt"),
                    function: "proto_deactivate".into(),
                })
                .unwrap(),
            shell
                .format_hook(Hook::RegisterHandlers {
                    function: "proto_activate".into(),
                })
                .unwrap(),
        ]
        .join("\n\n"),
    );
    sandbox.create_file(
        "test.mx",
        r#"
source ./hook.mx
proto_activate
out "active=$ENV.E2E_FOO events=${runtime --events -> [onPrompt] -> len}"
proto_deactivate
sh -c 'echo "torn-down=${E2E_FOO:-unset}"'
out "events=${runtime --events -> [onPrompt] -> len}"
proto_activate
sh -c 'echo "called-after=${E2E_FOO:-unset}"'
out done
"#,
    );

    let output = match Command::new("murex")
        .arg("./test.mx")
        .current_dir(sandbox.path())
        .output()
    {
        Ok(output) => output,
        Err(_) => {
            println!("murex not found on PATH, skipping");
            return;
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");

    // `proto_activate` after teardown falls through to an external command
    // that does not exist, which is an error murex reports and survives
    assert_eq!(
        stdout,
        "active=123 events=1\ntorn-down=unset\nevents=0\ncalled-after=unset\ndone\n"
    );
}
