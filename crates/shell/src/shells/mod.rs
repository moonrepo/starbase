mod bash;
mod elvish;
mod fish;
mod ion;
mod nu;
mod pwsh;
mod xonsh;
mod zsh;

pub use bash::*;
pub use elvish::*;
pub use fish::*;
pub use ion::*;
pub use nu::*;
pub use pwsh::*;
pub use xonsh::*;
pub use zsh::*;

use std::path::{Path, PathBuf};

pub trait Shell {
    /// Format an environment variable that will be exported to the entire shell,
    /// and be written to a profile file.
    fn format_env_export(&self, key: &str, value: &str) -> String;

    /// Format the provided paths to prepend the `PATH` environment variable,
    /// and be written to a profile file.
    fn format_path_export(&self, paths: &[String]) -> String;

    /// Return the path in which commands, aliases, and other settings will be configured.
    fn get_config_path(&self, home_dir: &Path) -> PathBuf;

    /// Return the path in which environment settings will be defined.
    fn get_env_path(&self, home_dir: &Path) -> PathBuf;

    /// Return a list of all possible interactive profile paths.
    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf>;
}

pub type BoxedShell = Box<dyn Shell>;
