use super::Shell;
use crate::helpers::get_config_dir;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Ion;

// https://doc.redox-os.org/ion-manual/general.html#xdg-app-dirs-support
impl Shell for Ion {
    fn get_main_profile_path(&self, home_dir: &Path) -> PathBuf {
        get_config_dir(home_dir).join("ion").join("initrc")
    }

    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf> {
        HashSet::<PathBuf>::from_iter([
            get_config_dir(home_dir).join("ion").join("initrc"),
            home_dir.join(".config").join("ion").join("initrc"),
        ])
        .into_iter()
        .collect()
    }
}
