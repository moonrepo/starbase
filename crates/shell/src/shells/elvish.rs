use super::Shell;
use crate::helpers::get_config_dir;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Elvish;

// https://elv.sh/ref/command.html#using-elvish-interactivelyn
impl Shell for Elvish {
    fn get_main_profile_path(&self, home_dir: &Path) -> PathBuf {
        get_config_dir(home_dir).join("elvish").join("rc.elv")
    }

    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf> {
        #[allow(unused_mut)]
        let mut profiles = HashSet::<PathBuf>::from_iter([
            get_config_dir(home_dir).join("elvish").join("rc.elv"),
            home_dir.join(".config").join("elvish").join("rc.elv"),
            home_dir.join(".elvish").join("rc.elv"), // Legacy
        ]);

        #[cfg(windows)]
        {
            profiles.insert(
                home_dir
                    .join("AppData")
                    .join("Roaming")
                    .join("elvish")
                    .join("rc.elv"),
            );
        }

        profiles.into_iter().collect()
    }
}
