use super::Shell;
use crate::helpers::{get_config_dir, get_env_var_regex};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Elvish;

fn format(value: impl AsRef<str>) -> String {
    get_env_var_regex()
        .replace_all(value.as_ref(), "$$E:$name")
        .replace("$E:HOME", "{~}")
}

// https://elv.sh/ref/command.html#using-elvish-interactivelyn
impl Shell for Elvish {
    fn format_env_export(&self, key: &str, value: &str) -> String {
        format!("set-env {key} {}", format(value))
    }

    fn format_path_export(&self, paths: &[String]) -> String {
        format!("set paths [{} $@paths]", format(paths.join(" ")))
    }

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
