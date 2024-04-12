use super::Shell;
use crate::helpers::get_config_dir;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Fish;

// https://fishshell.com/docs/current/language.html#configuration
impl Shell for Fish {
    fn format_env_export(&self, key: &str, value: &str) -> String {
        format!(r#"set -gx {key} "{value}""#)
    }

    fn format_path_export(&self, paths: &[String]) -> String {
        format!(r#"set -gx PATH "{}" $PATH"#, paths.join(":"))
    }

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
