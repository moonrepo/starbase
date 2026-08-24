pub enum Statement<'data> {
    ModifyPath {
        paths: &'data [String],
        key: Option<&'data str>,
        orig_key: Option<&'data str>,
    },
    SetAlias {
        name: &'data str,
        value: &'data str,
    },
    SetEnv {
        key: &'data str,
        value: &'data str,
    },
    UnsetAlias {
        name: &'data str,
    },
    UnsetEnv {
        key: &'data str,
    },
}

#[non_exhaustive]
pub enum Hook {
    /// Registers a function that evaluates a command's output whenever the
    /// shell's context changes — the working directory, or whatever state a
    /// new prompt reflects — alongside a paired function that reverses it and
    /// unregisters the triggers.
    ///
    /// A shell registers on every trigger it provides, and deactivation
    /// unregisters all of them:
    ///
    /// - Directory change and prompt: zsh (`chpwd_functions`,
    ///   `precmd_functions`), fish (`--on-variable PWD`,
    ///   `--on-event fish_prompt`), nu (`env_change.PWD`, `pre_prompt`),
    ///   elvish (`$after-chdir`, `$edit:before-readline`), xonsh
    ///   (`events.on_chdir`, `events.on_pre_prompt`), pwsh
    ///   (`LocationChangedAction`, wraps the global `prompt` function).
    /// - Prompt only: bash (`PROMPT_COMMAND`), murex (`onPrompt`), powershell
    ///   (wraps the global `prompt` function, as `LocationChangedAction`
    ///   requires PowerShell 6+).
    /// - On `cd` itself: sh, ash, dash — the `cd` builtin is shadowed with a
    ///   function, and deactivation restores it. This clobbers any other `cd`
    ///   wrapper, as POSIX offers no way to chain functions. These shells have
    ///   no prompt hook to register on.
    /// - Unsupported: ion.
    ///
    /// Registering both triggers means the activate command runs on every
    /// prompt as well as on every directory change, so a `cd` that returns to
    /// a prompt runs it twice. In exchange, the prompt trigger catches state
    /// that changed without a `cd`, and applies the activation to a session
    /// that sources the hook without ever changing directory.
    ///
    /// Prompt triggers only fire in an interactive session, so scripted use
    /// sees the directory trigger alone. Elvish goes further: its prompt
    /// trigger lives in the `edit:` module, which does not exist outside an
    /// interactive session, so that registration is skipped entirely there.
    ///
    /// Most shells evaluate the command's output as shell syntax. Nu cannot
    /// evaluate code at runtime, so both commands must instead print JSON:
    /// `{ "env": { "KEY": "value" | null }, "paths": ["..."], "path": "...",
    /// "aliases": { "name": "command" | null } }`, where a null value unsets
    /// the variable, `paths` sets `PATH` from a list, `path` sets it from a
    /// pre-joined string, and an alias is defined from a command line (not a
    /// string) or removed when null.
    ///
    /// Nu aliases are also parse time only, so they cannot be defined by the
    /// command that applies the rest of the data. They are instead staged as
    /// a `pre_prompt` hook, which nu parses in the scope that triggered it,
    /// and which drops itself once it has run. Aliases therefore apply at the
    /// next prompt rather than immediately, and not at all when scripted,
    /// where hooks never fire.
    ///
    /// Consumers typically append an invocation of the activate function for
    /// the initial run. This must include call parentheses for xonsh
    /// (`_hook()`), and be a bare word for every other shell. Elvish and
    /// xonsh evaluate the hook in a namespace of their own, so the appended
    /// call works because it is part of the same evaluated string, not
    /// because the function is reachable afterwards.
    ///
    /// Reaching both functions afterwards is what the shell scope must be
    /// told about, since neither `eval` nor `execx()` evaluate into it:
    ///
    /// - Xonsh: the generated code assigns both functions into
    ///   `__xonsh__.ctx`, and the deactivate function pops them again.
    /// - Elvish: the generated code passes both functions to `edit:add-vars`,
    ///   and the deactivate function removes them with `edit:del-vars`. The
    ///   `edit:` module only exists in an interactive session, so both calls
    ///   are wrapped in `try` and do nothing when scripted. Names added this
    ///   way become available in the next REPL cycle. A script that must call
    ///   the functions should evaluate the hook as a module instead
    ///   (`use <module>`), which namespaces them as `<module>:<function>`.
    OnContextChange {
        /// Command that prints statements to evaluate when activating,
        /// in shell specific-syntax (or JSON for nu), e.g. `proto activate zsh --export`.
        activate_command: String,

        /// Name of the function that evaluates [`Hook::OnContextChange::activate_command`],
        /// and is registered on every trigger the shell provides.
        activate_function: String,

        /// Command that prints statements to evaluate when deactivating,
        /// in shell specific-syntax (or JSON for nu), e.g. `proto deactivate zsh --export`.
        deactivate_command: String,

        /// Name of the user-callable function that deactivates the current session:
        /// evaluates [`Hook::OnContextChange::deactivate_command`], unregisters
        /// every trigger, and removes both functions where the shell allows it.
        deactivate_function: String,
    },
}

impl Hook {
    pub fn get_info(&self) -> &str {
        match self {
            Hook::OnContextChange { .. } => "on context change",
        }
    }
}
