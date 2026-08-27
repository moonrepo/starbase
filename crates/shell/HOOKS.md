# Shell hooks

How `starbase_shell` activates a tool inside a live shell session, keeps it
active as the session's context changes, and tears it down again. This
document records the design, the per-shell mechanics, and the reasoning behind
the decisions — including the approaches that were tried and rejected, and the
bugs that shaped the current shape.

Everything here was verified by executing generated code in real shells (see
[Testing](#testing)), except where a section says otherwise.

## The model

A consumer (proto is the canonical one) wants a session where entering a
directory applies environment changes, and leaving it reverts them. That
breaks down into four independent pieces, one `Hook` variant each:

| Hook | Renders | Parameterized by |
| --- | --- | --- |
| `Activate` | A function that evaluates a command's output | `command`, `function` |
| `Deactivate` | The same, under another name | `command`, `function` |
| `RegisterHandlers` | Code registering a function on every trigger the shell has | `function` |
| `UnregisterHandlers` | Code removing exactly what `RegisterHandlers` added | `function` |

`Activate` and `Deactivate` are the *same template*: a deactivate function is
an activate function whose command prints the opposite statements. A test
(`deactivate_matches_activate`) pins this — anything that drifts between them
is a bug.

The canonical composition, using proto's names:

```
Shell profile evaluates the output of `proto activate <shell>`, which is:
    Activate            → defines proto_activate
    Deactivate          → defines proto_deactivate
    RegisterHandlers    → registers proto_activate on cd + prompt triggers

Trigger fires (cd, prompt) → proto_activate runs
    → evaluates `proto activate --export` output: env vars, aliases

User runs proto_deactivate → evaluates `proto deactivate --export` output:
    1. statements unsetting env vars and aliases
    2. the rendered UnregisterHandlers hook
    3. UnsetFunction statements for proto_activate and proto_deactivate
```

The teardown ordering matters and only one order is safe: **unregister first,
then delete the functions**. The reverse leaves a window where a trigger fires
into a deleted name (see [Why deleting functions is not
teardown](#why-deleting-functions-is-not-teardown)).

Two properties are deliberate:

- **`Deactivate` does not unregister.** It is pure reversal. A session that
  deactivates without unregistering keeps triggering, and the command is
  expected to print nothing once torn down — or, the normal case, the
  deactivate command's output carries the `UnregisterHandlers` code as in the
  composition above.
- **`UnregisterHandlers` does not delete the functions.** Nu cannot undefine
  commands at runtime, so deleting-on-unregister would leave that shell half
  torn down. Function removal is a separate `Statement::UnsetFunction`, issued
  by the consumer.

## Triggers per shell

A shell registers on *every* trigger it provides. Registration is idempotent
(evaluating `RegisterHandlers` twice leaves one registration), and
unregistering something never registered is a no-op.

| Shell | Directory change | Prompt |
| --- | --- | --- |
| zsh | `chpwd_functions` | `precmd_functions` |
| fish | `--on-variable PWD` | `--on-event fish_prompt` |
| nu | `hooks.env_change.PWD` | `hooks.pre_prompt` |
| elvish | `$after-chdir` | `$edit:before-readline` |
| xonsh | `events.on_chdir` | `events.on_pre_prompt` |
| pwsh | `LocationChangedAction` | wraps global `prompt` |
| bash | — | `PROMPT_COMMAND` |
| murex | — | `onPrompt` event |
| powershell | — | wraps global `prompt` (`LocationChangedAction` needs 6+) |
| sh, ash, dash | shadows the `cd` builtin | — |
| ion | unsupported | unsupported |

Consequences:

- A shell with both triggers runs the activate command twice per `cd` (the
  prompt follows the directory change). In exchange, the prompt trigger
  catches state that changed without a `cd`, and sourcing the hook activates
  the session before the user types anything.
- Prompt triggers never fire outside an interactive session, so scripts see
  the directory trigger alone. In nu, *nothing* fires in a script.
- The POSIX `cd` shadow clobbers any other `cd` wrapper — POSIX has no way to
  chain functions — and `unset -f cd` on unregister restores the builtin,
  dropping any wrapper installed after ours (see
  [Restore-ordering hazard](#restore-ordering-hazard)).
- fish binds events to a function definition, so the trigger is a separate
  wrapper (`<function>_on_context`) rather than an attribute of the activate
  function; erasing the wrapper unbinds both events with it.

## Statements

`Statement` renders individual changes: `ModifyPath`, `SetEnv` / `UnsetEnv`,
`SetAlias` / `UnsetAlias`, `SetFunction` / `UnsetFunction`. Every variant
carries a `hook: bool` selecting between two syntaxes:

- `hook: false` — for a profile, an rc file, or the top level of a session.
- `hook: true` — for statements a hook function will evaluate.

The two only differ where a shell treats hook-evaluated code differently.
Today that is **elvish aliases and functions** and **xonsh functions**; every
other statement is context-free, and the flag exists on all variants so the
enum never changes shape when another shell diverges. The trait helpers
(`format_alias_set`, `format_function_set`, …) are the `hook: false`
conveniences; construct the statement directly for the hook form.

Two statement classes have burned us and are worth naming:

- **Assignment parsers.** Nu and murex parse the right side of `=` as an
  expression, and powershell as a pipeline — a bareword there is a syntax
  error (nu, murex) or runs as a *command* (powershell: `$env:X = true`
  resolves `/usr/bin/true` and silently assigns empty on Unix). Plain
  alphanumeric values, the ones that look safest, are exactly the ones that
  break. All three quote unconditionally now, and
  `expression_shells_quote_a_plain_env_value` guards the class.
- **Tolerant removal.** Unsetting something that does not exist must never
  abort the surrounding statement block, because statements are evaluated as
  one unit. xonsh uses `pop(..., None)` instead of `del`; elvish's hook forms
  use `edit:del-vars` instead of `del` (a *compilation* error on a missing
  name, which kills every statement sharing the `eval`).

## Per-shell mechanics

### elvish

The most constrained shell here; most of the design pressure came from it.

- **`eval` evaluates in a restricted namespace that is thrown away.**
  Functions and aliases defined by evaluated statements vanish when `eval`
  returns. Everything user-callable must be exported through `edit:add-vars`,
  and removal goes through `edit:del-vars`. Both live in the `edit:` module,
  which **only exists in an interactive session** — so exports are wrapped in
  `try`, scripted sessions get no functions or aliases at all, and names land
  in the **next REPL cycle**, not the current one.
- **Referencing `edit:` (or `$edit:before-readline`) directly is a
  compilation error** when the module is absent, and `try` cannot catch
  compilation errors. Any code touching it is compiled at runtime inside
  `eval '...'`, inside the `try`.
- **`RegisterHandlers` must be evaluated in the same `eval` as `Activate`.**
  It references `$<function>~` by identity, and a separate eval cannot see
  it. The composed activation output satisfies this naturally.
- **`UnregisterHandlers` must NOT reference the function.** It can be
  evaluated inside the deactivate function, whose `eval` namespace contains
  only what the function body captured — nothing. An identity reference there
  is a compilation error that kills the *entire* deactivate output, env
  resets included (this shipped as a bug and was caught by the lifecycle
  test). Instead, the `Activate` template stamps `# <function>` as the first
  body line, and unregistration matches handlers by that marker in their
  `def` text (`str:contains $handler[def] "# <function>\n"`) — context-free
  by construction.
- **Function scoping is lexical and capture-based.** A function body only
  captures the upvalues it references. This is also why a
  "wrap the unregister code in a function and call it later" design fails:
  the wrapper is callable at the prompt (via `edit:add-vars`) but a bareword
  call inside the deactivate function's eval resolves to an external command
  and aborts the rest of the stream. Verified empirically; do not revisit
  without re-testing.

### xonsh

- **`execx()` does not evaluate into the shell's namespace.** Definitions
  land in a throwaway exec namespace; the shell can only reach what the
  generated code assigns into `__xonsh__.ctx`. The templates export the
  activate/deactivate functions there, `RegisterHandlers` reads the function
  from `ctx` (it may have been defined by a *different* `execx`), and
  `SetFunction { hook: true }` appends the same export.
- In a **script**, top-level `globals()` *is* `__xonsh__.ctx`, so the same
  code works in both contexts.
- Unregistration matches handlers by `__name__` across both events. Removal
  of names uses `ctx.pop(name, None)` — never `del`, which raises.

### nu

Nu has no runtime `eval`; nothing can evaluate a string of statements. The
workaround defines the whole activate/deactivate mechanism:

- The activate function writes the command's output to a temp file —
  `$nu.temp-dir/<function>-<pid>.nu`, keyed by pid so concurrent sessions
  cannot cross-read, and by function so two tools cannot clobber each other —
  and stages a **one-shot `pre_prompt` entry** that `source`s the file and
  removes itself. Only a hook entry defined as a *string* is parsed in the
  scope that triggered it, which is what lets the sourced statements take
  effect in the session.
- `source` requires a parse-time constant path, so the path is baked into the
  staged entry as a `const`.
- Consequences: statements land at the **next prompt**, not at the call; and
  a failing command stages nothing rather than aborting the prompt.
- **Unregistration purges pending staged entries too** (matched by their
  `# <function> apply` first line). Without this there is a resurrection
  race: on the prompt where a staged deactivation applies, the
  still-registered handler has already staged a fresh activation, which would
  re-apply one prompt after teardown. This was observed as a flaky test
  before the purge existed.
- Nu **cannot undefine commands**. Deactivated functions survive; calling one
  writes a staged file that the next activation overwrites before it can
  apply, so it is harmless. `UnsetFunction` renders `hide`, which makes the
  name unresolvable in the session.
- Aliases are parse-time keywords (`alias`, `hide`) that no command can run;
  they ride the same staged-file mechanism.

### POSIX (sh, ash, dash)

No hooks of any kind. The `cd` builtin is shadowed by a function that runs
the real `cd` and then the activate function. ash and dash delegate to the sh
implementation. Nothing fires on prompts, so activation happens on the next
`cd` only.

### PowerShell (powershell, pwsh)

- Both wrap the global `prompt` function, saving the previous one in a global
  variable and restoring it on unregister. pwsh additionally registers a
  `LocationChangedAction` delegate (5.1 lacks it).
- The activate template preserves `$LASTEXITCODE` across the hook, so a
  prompt refresh does not clobber the user's last exit status.
- `SetFunction` renders `function global:<name>` — a plain `function` defined
  by hook-evaluated code would die with the hook function's scope.
- These are the only implementations never executed on the development
  machine (no PowerShell installed); CI is their first run. Everything is
  snapshot-tested and mirrors the mechanisms above.

### murex

`onPrompt` event, `-> source` for evaluation, `!event` / `!function` for
removal. Nothing exotic — but murex's line editor cannot be driven through a
pty (it swallows the first Enter of a session and appends later input to the
line it is still holding), so its interactive coverage runs as a script
instead, and one pty test is `#[ignore]`d with this explanation.

### ion

No hook support at all — every `Hook` renders `ShellError::NoHookSupport`.
Ion also has no way to remove a function, so `UnsetFunction` renders a
comment rather than a statement that would fail.

## Deactivation: the decisions

### Why deleting functions is not teardown

"If we just delete the functions, do the handlers disappear?" was measured
directly. Three behaviors exist, and only one shell has the nice one:

| Behavior | Shells |
| --- | --- |
| Handler **keeps firing successfully** — deletion changes nothing | elvish, xonsh (triggers hold the function *object*, not the name) |
| **Error on every trigger** for the rest of the session | bash, fish, nu, murex, sh/ash/dash, powershell/pwsh |
| Silently skipped | zsh only (its hook driver checks existence) |

So deletion-as-teardown is a zsh-ism. In elvish and xonsh the session keeps
re-activating as if nothing happened; everywhere else the user gets an error
printed at every prompt or `cd`. Hence: unregister first, delete second — and
after unregistering, deletion is safe everywhere.

### Unregister vs gate (the mise comparison)

mise, facing the same problem, mixes two strategies per shell: it genuinely
unregisters in zsh/bash/xonsh (and fish, where erasing the function *is*
unregistration), but in elvish/nushell/pwsh it leaves handlers registered
forever and gates the hook body on state its deactivation clears
(`$hook-enabled`, `"MISE_SHELL" in $env`, `$env:MISE_SHELL -eq "pwsh"`).
Notably it gates in exactly the shells where unregistration is hard.

This crate unregisters everywhere, because the hard shells turned out to be
solvable (elvish's marker matching, nu's staged purge), and unregistering has
strictly better end-state properties: a torn-down session carries **zero
residue and zero per-prompt cost**, where a gated handler fires its check on
every prompt forever, and re-activation must cope with stacked state (mise's
activation begins by running its full deactivation script for this reason).

Gating remains available *to consumers* with no crate support: a deactivated
`proto activate --export` that prints nothing is a gate, and composes with —
or substitutes for — `UnregisterHandlers`. Prefer the shell-side gate only
when avoiding the per-prompt process spawn does not matter.

### Restore-ordering hazard

Most unregistrations remove an entry from a list, which cannot affect any
other tool. The exceptions restore saved state: POSIX drops the `cd` shadow,
and powershell/pwsh restore the prompt function saved at registration. If
another tool wrapped `cd` or the prompt *after* this one, that restore drops
their wrapper too. Unregister before other tools' teardown, or after their
setup, when stacking matters. (Gating avoids this hazard entirely, which is
its one structural advantage.)

## Templates

Hook code lives in plain files under `src/shells/hooks/`, one directory per
hook kind:

```
hooks/
  function/     Activate and Deactivate (shared template)
  register/     RegisterHandlers
  unregister/   UnregisterHandlers
```

Ten files each (`bash.bash`, `zsh.zsh`, `fish.fish`, `sh.sh`, `murex.mx`,
`nu.nu`, `elvish.elv`, `xonsh.xsh`, `powershell.ps1`, `pwsh.ps1`); ash and
dash delegate to sh in Rust. Files are pulled in with `include_str!` and
rendered by `render_template` in `helpers.rs`:

- Placeholders are GitHub-Actions style: `${{ function }}`, `${{ command }}`.
  Whitespace-tolerant. Everything else — braces included — is copied
  verbatim, which is the point: no `{{` doubling, no raw-string `"#`
  hazards, real syntax highlighting.
- An unresolved placeholder (unknown key, unterminated) is re-emitted
  verbatim, never dropped. Since templates render at runtime, a typo cannot
  fail the build the way `format!` arguments did — instead
  `no_shell_leaves_template_placeholders` renders every hook for every shell
  and asserts no `${{` survives. That test demonstrably catches a
  single-character placeholder typo.
- Nu is the one shell whose template takes computed values (`file`,
  `marker`, `staged`, …) built in `nu.rs`, since parts of its staged entry
  are themselves generated.
- A literal `$` immediately before a placeholder is fine: sh renders
  `$${{ function }}_output` → `$_activate_output`.

## Testing

Four layers, each covering what the previous cannot:

1. **Unit + snapshots** (`src/shells/*.rs`): every hook and statement
   rendering, snapshot per shell per hook. Includes the cross-shell guards:
   `deactivate_matches_activate`, `no_shell_leaves_template_placeholders`,
   `expression_shells_quote_a_plain_env_value`.
2. **Non-interactive E2E** (`tests/hook_e2e_test.rs`): generated hooks
   executed by the real shell binaries as scripts. Reaches directory
   triggers, registration counts, teardown — everything a script can see.
3. **Interactive E2E** (`tests/hook_interactive_test.rs` +
   `tests/pty/mod.rs`): a minimal pty harness (libc `openpty`, own session +
   controlling tty) driving real interactive shells. This is the only place
   prompt triggers, elvish's `edit:` module, and nu's staged entries are
   observable. Harness essentials, each learned the hard way: send Enter as
   `\r` (raw-mode line editors never see the `\n` translation); answer
   `ESC[6n` cursor-position queries or line editors hang; `TERM` per shell
   (some editors need capabilities, others hang asking); marker-driven
   `sync` that re-sends until the step's own output appears, because a
   still-starting shell silently drops typed input.
4. **Lifecycle** (`tests/proto_flow_test.rs`): the full composition exactly
   as proto ships it — teardown statements arriving via the deactivate
   command's *output*, evaluated inside the deactivate function. This is the
   layer that caught both context bugs (elvish identity reference, nu
   resurrection race): the other layers evaluated `UnregisterHandlers` at the
   top level, where both bugs are invisible.

Environment knobs: a missing shell skips its tests unless named in
`STARBASE_REQUIRED_SHELLS` (comma-separated; CI sets it per OS so a broken
install fails loudly). `STARBASE_PTY_TIMEOUT` overrides the pty marker
timeout in seconds.

Known gaps: powershell/pwsh execute only on CI; murex has no pty coverage
(scripted lifecycle instead); ion renders errors by design.
