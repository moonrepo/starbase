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

pub enum Hook {
    OnChangeDir {
        /// Command that prints statements to evaluate when activating,
        /// in shell specific-syntax (or JSON for nu), e.g. `proto activate zsh --export`.
        activate_command: String,

        /// Name of the function that evaluates [`Hook::OnChangeDir::activate_command`],
        /// and is registered on the shell's change-dir (or prompt) trigger.
        activate_function: String,

        /// Command that prints statements to evaluate when deactivating,
        /// in shell specific-syntax (or JSON for nu), e.g. `proto deactivate zsh --export`.
        deactivate_command: String,

        /// Name of the user-callable function that deactivates the current session:
        /// evaluates [`Hook::OnChangeDir::deactivate_command`], unregisters the
        /// change-dir trigger, and removes both functions where the shell allows it.
        deactivate_function: String,
    },
}

impl Hook {
    pub fn get_info(&self) -> &str {
        match self {
            Hook::OnChangeDir { .. } => "on change directory",
        }
    }
}
