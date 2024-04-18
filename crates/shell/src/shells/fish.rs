use super::Shell;
use crate::helpers::get_config_dir;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Fish;

impl Fish {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self
    }
}

// https://fishshell.com/docs/current/language.html#configuration
impl Shell for Fish {
    fn format_env_export(&self, key: &str, value: &str) -> String {
        format!(r#"set -gx {key} "{value}""#)
    }

    fn format_path_export(&self, paths: &[String]) -> String {
        format!(r#"set -gx PATH "{}" $PATH"#, paths.join(":"))
    }

    fn get_config_path(&self, home_dir: &Path) -> PathBuf {
        get_config_dir(home_dir).join("fish").join("config.fish")
    }

    fn get_env_path(&self, home_dir: &Path) -> PathBuf {
        self.get_config_path(home_dir)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_env_var() {
        assert_eq!(
            Fish.format_env_export("PROTO_HOME", "$HOME/.proto"),
            r#"set -gx PROTO_HOME "$HOME/.proto""#
        );
    }

    #[test]
    fn formats_path() {
        assert_eq!(
            Fish.format_path_export(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"set -gx PATH "$PROTO_HOME/shims:$PROTO_HOME/bin" $PATH"#
        );
    }
}
