mod bash;
mod elvish;
mod fish;
mod ion;
mod nu;
mod pwsh;
mod zsh;

pub use bash::*;
pub use elvish::*;
pub use fish::*;
pub use ion::*;
pub use nu::*;
pub use pwsh::*;
pub use zsh::*;

use std::path::{Path, PathBuf};

pub trait Shell {
    /// Format an environment variable that will be exported to the entire shell,
    /// and be written to a profile file.
    fn format_env_export(&self, key: &str, value: &str) -> String;

    /// Format the provided paths to prepend the `PATH` environment variable,
    /// and be written to a profile file.
    fn format_path_export(&self, paths: &[String]) -> String;

    /// Return the profile path that should be used for interactive shells.
    /// This is also the profile that environment variables will be written to.
    fn get_main_profile_path(&self, home_dir: &Path) -> PathBuf;

    /// Return a list of all possible interactive profile paths.
    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf>;
}

pub type BoxedShell = Box<dyn Shell>;
