mod bash;
mod elvish;
mod fish;
mod ion;
mod nu;
mod pwsh;
mod sh;
mod xonsh;
mod zsh;

pub use bash::*;
pub use elvish::*;
pub use fish::*;
pub use ion::*;
pub use nu::*;
pub use pwsh::*;
pub use sh::*;
pub use xonsh::*;
pub use zsh::*;

use std::ffi::OsString;
use std::fmt::Display;
use std::path::{Path, PathBuf};

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

pub trait Shell: Display {
    /// Format an environment variable that will be set to the entire shell,
    /// and be written to a profile file.
    fn format_env_set(&self, key: &str, value: &str) -> String;

    /// Format an environment variable that will be unset from the entire shell,
    /// and be written to a profile file.
    fn format_env_unset(&self, key: &str) -> String;

    /// Format the provided paths to prepend the `PATH` environment variable,
    /// and be written to a profile file.
    fn format_path_set(&self, paths: &[String]) -> String;

    /// Return the path in which commands, aliases, and other settings will be configured.
    fn get_config_path(&self, home_dir: &Path) -> PathBuf;

    /// Return the path in which environment settings will be defined.
    fn get_env_path(&self, home_dir: &Path) -> PathBuf;

    /// Return parameters for executing a one-off command and then exiting.
    fn get_exec_command(&self) -> ShellCommand {
        ShellCommand::default()
    }

    /// Return a list of all possible interactive profile paths.
    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf>;
}

pub type BoxedShell = Box<dyn Shell>;
