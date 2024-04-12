use super::Shell;
use crate::helpers::get_config_dir;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Ion;

impl Shell for Ion {
    // https://doc.redox-os.org/ion-manual/variables/05-exporting.html
    fn format_env_export(&self, key: &str, value: &str) -> String {
        format!(r#"export {key} = "{value}""#)
    }

    fn format_path_export(&self, paths: &[String]) -> String {
        // TODO Not sure if correct
        format!(r#"export PATH = "{}:{}""#, paths.join(":"), "${env::PATH}")
    }

    fn get_main_profile_path(&self, home_dir: &Path) -> PathBuf {
        get_config_dir(home_dir).join("ion").join("initrc")
    }

    // https://doc.redox-os.org/ion-manual/general.html#xdg-app-dirs-support
    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf> {
        HashSet::<PathBuf>::from_iter([
            get_config_dir(home_dir).join("ion").join("initrc"),
            home_dir.join(".config").join("ion").join("initrc"),
        ])
        .into_iter()
        .collect()
    }
}
