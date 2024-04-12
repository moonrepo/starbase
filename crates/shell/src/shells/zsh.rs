use super::Shell;
use crate::helpers::is_absolute_dir;
use std::env;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Zsh;

// https://zsh.sourceforge.io/Doc/Release/Files.html#Files
impl Shell for Zsh {
    fn get_main_profile_path(&self, home_dir: &Path) -> PathBuf {
        env::var_os("ZDOTDIR")
            .and_then(is_absolute_dir)
            .unwrap_or(home_dir.to_owned())
            .join(".zprofile")
    }

    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf> {
        let zdot_dir = env::var_os("ZDOTDIR")
            .and_then(is_absolute_dir)
            .unwrap_or(home_dir.to_owned());

        vec![zdot_dir.join(".zprofile"), zdot_dir.join(".zshrc")]
    }
}
