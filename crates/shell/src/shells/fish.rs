use super::Shell;
use crate::helpers::get_config_dir;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Fish;

// https://fishshell.com/docs/current/language.html#configuration
impl Shell for Fish {
    fn get_main_profile_path(&self, home_dir: &Path) -> PathBuf {
        get_config_dir(home_dir).join("fish").join("config.fish")
    }

    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf> {
        HashSet::<PathBuf>::from_iter([
            get_config_dir(home_dir).join("fish").join("config.fish"),
            home_dir.join(".config").join("fish").join("config.fish"),
        ])
        .into_iter()
        .collect()
    }
}
