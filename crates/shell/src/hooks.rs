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
    /// Every shell evaluates the command's output as its own syntax. Nu has no
    /// runtime `eval`, so it stages that output in a file and applies it with
    /// `source`, which parses and runs the file in the scope that triggered the
    /// hook. Each trigger therefore registers two entries: the activate
    /// function, which writes the file, and a `source` entry that reads it. Nu
    /// parses a hook entry defined as a string immediately before running it,
    /// so the second entry always sees what the first just wrote. This is also
    /// what makes aliases work, `alias` and `hide` being parse time keywords
    /// that no command can run.
    ///
    /// The staged file lives in `$nu.temp-dir`, keyed by pid and by activate
    /// function name, so that concurrent sessions and separate tools cannot
    /// read each other's statements. Its path must be a parse time constant,
    /// as `source` rejects a runtime one. Keying by pid is what keeps two
    /// sessions from applying each other's statements, the window between the
    /// write and the `source` being wide enough to collide in practice. The
    /// `source` entry deletes the file once it has applied it, so nothing is
    /// left behind for a session exit to orphan, and nu has no exit hook to
    /// clean up with. Deleting it is safe because the writer entry runs first
    /// on every trigger, recreating it before the `source` entry is parsed.
    ///
    /// Two consequences are specific to nu. Deactivation is staged the same
    /// way, since no command can evaluate shell syntax either, so it lands on
    /// the trigger that follows the call rather than immediately, and the
    /// staged teardown unregisters the `source` entry as its last act. And
    /// because every statement is applied by a hook, a non-interactive nu
    /// never activates: `pre_prompt` does not fire there, and calling the
    /// activate function only writes the file.
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
    ///
    /// Elvish aliases ride the same mechanism, since a function defined by the
    /// evaluated statements would be dropped with the namespace that `eval`
    /// discards. Two consequences are user visible: an alias set on entering a
    /// directory becomes callable at the following prompt rather than
    /// immediately, and a non-interactive elvish gets no aliases at all, the
    /// same way it gets no hook functions.
    OnContextChange {
        /// Command that prints statements to evaluate when activating,
        /// in shell specific-syntax, e.g. `proto activate zsh --export`.
        activate_command: String,

        /// Name of the function that evaluates [`Hook::OnContextChange::activate_command`],
        /// and is registered on every trigger the shell provides.
        activate_function: String,

        /// Command that prints statements to evaluate when deactivating,
        /// in shell specific-syntax, e.g. `proto deactivate zsh --export`.
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
