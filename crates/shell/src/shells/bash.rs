use super::Shell;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Bash;

// https://www.baeldung.com/linux/bashrc-vs-bash-profile-vs-profile
impl Shell for Bash {
    fn format_env_export(&self, key: &str, value: &str) -> String {
        format!(r#"export {key}="{value}""#)
    }

    fn format_path_export(&self, paths: &[String]) -> String {
        format!(r#"export PATH="{}:$PATH""#, paths.join(":"))
    }

    fn get_main_profile_path(&self, home_dir: &Path) -> PathBuf {
        home_dir.join(".bash_profile")
    }

    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf> {
        vec![
            home_dir.join(".bash_profile"),
            home_dir.join(".bashrc"),
            home_dir.join(".profile"),
        ]
    }
}
