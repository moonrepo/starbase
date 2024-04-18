use super::Shell;
use crate::helpers::get_config_dir;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Xonsh;

impl Xonsh {
    pub fn new() -> Self {
        Self
    }
}

// https://xon.sh/bash_to_xsh.html
// https://xon.sh/xonshrc.html
impl Shell for Xonsh {
    fn format_env_export(&self, key: &str, value: &str) -> String {
        format!(r#"${key} = "{value}""#)
    }

    fn format_path_export(&self, paths: &[String]) -> String {
        format!(r#"$PATH = "{}:$PATH""#, paths.join(":"))
    }

    fn get_config_path(&self, home_dir: &Path) -> PathBuf {
        get_config_dir(home_dir).join("xonsh").join("rc.xsh")
    }

    fn get_env_path(&self, home_dir: &Path) -> PathBuf {
        self.get_config_path(home_dir)
    }

    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf> {
        HashSet::<PathBuf>::from_iter([
            get_config_dir(home_dir).join("xonsh").join("rc.xsh"),
            home_dir.join(".config").join("xonsh").join("rc.xsh"),
            home_dir.join(".xonshrc"),
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
            Xonsh.format_env_export("PROTO_HOME", "$HOME/.proto"),
            r#"$PROTO_HOME = "$HOME/.proto""#
        );
    }

    #[test]
    fn formats_path() {
        assert_eq!(
            Xonsh.format_path_export(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"$PATH = "$PROTO_HOME/shims:$PROTO_HOME/bin:$PATH""#
        );
    }
}
