/// A change to apply to a shell session.
///
/// Every variant carries a `hook` flag, which selects between the syntax a
/// [`Hook`] needs and the syntax everything else needs — a profile, an rc
/// file, or output a session evaluates at the top level. The two only differ
/// where a shell treats hook evaluated code differently, which today is elvish
/// aliases: a hook evaluates its statements in a namespace that is thrown
/// away, so an alias has to be added to the interactive namespace rather than
/// defined with `fn`. The flag is carried by every variant so that another
/// shell can diverge later without the enum changing shape again.
pub enum Statement<'data> {
    ModifyPath {
        paths: &'data [String],
        key: Option<&'data str>,
        orig_key: Option<&'data str>,
        /// Format for embedding in a [`Hook`]. See [`Statement`].
        hook: bool,
    },
    SetAlias {
        name: &'data str,
        value: &'data str,
        /// Format for embedding in a [`Hook`]. See [`Statement`].
        hook: bool,
    },
    SetEnv {
        key: &'data str,
        value: &'data str,
        /// Format for embedding in a [`Hook`]. See [`Statement`].
        hook: bool,
    },
    UnsetAlias {
        name: &'data str,
        /// Format for embedding in a [`Hook`]. See [`Statement`].
        hook: bool,
    },
    UnsetEnv {
        key: &'data str,
        /// Format for embedding in a [`Hook`]. See [`Statement`].
        hook: bool,
    },
}

#[non_exhaustive]
pub enum Hook {
    /// Defines a function that evaluates a command's output, applying whatever
    /// statements it prints to the current session.
    ///
    /// Most shells evaluate the output as shell syntax. Nu cannot evaluate code
    /// at runtime, so the statements are written to a file and applied by a
    /// `source` entry staged on the next prompt, which delays them by a prompt
    /// and does nothing outside an interactive session.
    ///
    /// Elvish and xonsh evaluate the hook in a namespace of their own, so the
    /// generated code exports the function to the session: xonsh assigns it
    /// into `__xonsh__.ctx`, and elvish passes it to `edit:add-vars`, which
    /// only exists in an interactive session and takes effect in the next REPL
    /// cycle. In elvish this also means [`Hook::OnContextChange`] must be
    /// evaluated in the same namespace as the definition it registers, since a
    /// separate `eval` cannot see it.
    Activate {
        /// Command that prints statements to evaluate, in shell specific
        /// syntax, e.g. `proto activate zsh --export`.
        command: String,

        /// Name of the function to define.
        function: String,
    },

    /// Defines a function that reverses [`Hook::Activate`], by evaluating the
    /// output of a command that prints the opposite statements.
    ///
    /// This is [`Hook::Activate`] with another name and another command. It
    /// does not unregister [`Hook::OnContextChange`], so a shell that has
    /// registered one keeps triggering afterwards, and the command is expected
    /// to account for that by printing nothing once a session is torn down.
    Deactivate {
        /// Command that prints statements to evaluate, in shell specific
        /// syntax, e.g. `proto deactivate zsh --export`.
        command: String,

        /// Name of the function to define.
        function: String,
    },

    /// Registers a function on every trigger the shell provides, so that it
    /// runs whenever the session's context changes — the working directory, or
    /// whatever state a new prompt reflects.
    ///
    /// Registering is idempotent, so evaluating this more than once in a
    /// session leaves a single registration behind.
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
    ///   function. This clobbers any other `cd` wrapper, as POSIX offers no way
    ///   to chain functions.
    /// - Unsupported: ion.
    ///
    /// Registering both triggers means the command runs on every prompt as
    /// well as on every directory change, so a `cd` that returns to a prompt
    /// runs it twice. In exchange, the prompt trigger catches state that
    /// changed without a `cd`.
    ///
    /// Prompt triggers only fire in an interactive session, so scripted use
    /// sees the directory trigger alone. Elvish goes further: its prompt
    /// trigger lives in the `edit:` module, which does not exist outside an
    /// interactive session, so that registration is skipped entirely there.
    OnContextChange {
        /// Name of the function to register, as defined by [`Hook::Activate`].
        function: String,
    },
}

impl Hook {
    pub fn get_info(&self) -> &str {
        match self {
            Hook::Activate { .. } => "activate",
            Hook::Deactivate { .. } => "deactivate",
            Hook::OnContextChange { .. } => "on context change",
        }
    }
}
