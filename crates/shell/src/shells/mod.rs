mod ash;
mod bash;
mod elvish;
mod fish;
mod ion;
mod murex;
mod nu;
mod powershell;
mod pwsh;
mod sh;
mod xonsh;
mod zsh;

pub use ash::*;
pub use bash::*;
pub use elvish::*;
pub use fish::*;
pub use ion::*;
pub use murex::*;
pub use nu::*;
pub use powershell::*;
pub use pwsh::*;
pub use sh::*;
pub use xonsh::*;
pub use zsh::*;

use crate::helpers::get_var_regex;
use crate::hooks::*;
use crate::shell_error::ShellError;
use std::ffi::OsString;
use std::fmt::{Debug, Display};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct ShellCommand {
    pub shell_args: Vec<OsString>,
    pub pass_args_stdin: bool,
}

impl Default for ShellCommand {
    fn default() -> Self {
        // This is pretty much the same for all shells except pwsh.
        // bash -c "command", nu -c "command", etc...
        Self {
            shell_args: vec![OsString::from("-c")],
            pass_args_stdin: false,
        }
    }
}

pub trait Shell: Debug + Display + Send + Sync {
    /// Format the provided statement.
    fn format(&self, statement: Statement<'_>) -> String;

    /// Format an environment variable by either setting or unsetting the value.
    fn format_env(&self, key: &str, value: Option<&str>) -> String {
        match value {
            Some(value) => self.format_env_set(key, value),
            None => self.format_env_unset(key),
        }
    }

    /// Format an environment variable that will be set to the entire shell.
    fn format_env_set(&self, key: &str, value: &str) -> String {
        self.format(Statement::SetEnv { key, value })
    }

    /// Format an environment variable that will be unset from the entire shell.
    fn format_env_unset(&self, key: &str) -> String {
        self.format(Statement::UnsetEnv { key })
    }

    /// Format the provided paths to prepend the `PATH` environment variable.
    fn format_path_prepend(&self, paths: &[String]) -> String {
        self.format(Statement::ModifyPath {
            paths,
            key: Some("PATH"),
            orig_key: Some("PATH"),
        })
    }

    /// Format the provided paths to override the `PATH` environment variable.
    fn format_path_set(&self, paths: &[String]) -> String {
        self.format(Statement::ModifyPath {
            paths,
            key: Some("PATH"),
            orig_key: None,
        })
    }

    /// Format a hook for the current shell.
    fn format_hook(&self, hook: Hook) -> Result<String, ShellError> {
        Err(ShellError::NoHookSupport {
            name: self.to_string(),
            info: hook.get_info().to_owned(),
        })
    }

    /// Return the path in which commands, aliases, and other settings will be configured.
    fn get_config_path(&self, home_dir: &Path) -> PathBuf;

    /// Return the path in which environment settings will be defined.
    fn get_env_path(&self, home_dir: &Path) -> PathBuf;

    /// Return parameters for executing a one-off command and then exiting.
    fn get_exec_command(&self) -> ShellCommand {
        ShellCommand::default()
    }

    /// Return a list of all possible profile/rc/config paths.
    /// Ordered from most to least common/applicable.
    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf>;

    /// Quote method for shell-specific quoting
    fn quote(&self, value: &str) -> String;

    /// Return true if the provided string requires expansion.
    fn requires_expansion(&self, value: &str) -> bool {
        // https://www.gnu.org/software/bash/manual/bash.html#Shell-Expansions
        for ch in [
            "{", "}", // brace
            "~+", "~-", // tilde
            "${", // param
            "$(", // command
            "<(", ">(", // process
            "**", "*", "?", "?(", "*(", "+(", "@(", "!(", // file
        ] {
            if value.contains(ch) {
                return true;
            }
        }

        get_var_regex().is_match(value)
    }
}

pub type BoxedShell = Box<dyn Shell>;
