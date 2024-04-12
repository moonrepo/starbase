use super::Shell;
use crate::helpers::get_config_dir;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Nu;

// https://www.nushell.sh/book/configuration.html
impl Shell for Nu {
    fn get_main_profile_path(&self, home_dir: &Path) -> PathBuf {
        get_config_dir(home_dir).join("nushell").join("config.nu")
    }

    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf> {
        HashSet::<PathBuf>::from_iter([
            get_config_dir(home_dir).join("nushell").join("config.nu"),
            home_dir.join(".config").join("nushell").join("config.nu"),
        ])
        .into_iter()
        .collect()
    }
}
