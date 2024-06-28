use super::Shell;
use crate::helpers::get_config_dir;
use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Xonsh;

impl Xonsh {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self
    }
}

// https://xon.sh/bash_to_xsh.html
// https://xon.sh/xonshrc.html
impl Shell for Xonsh {
    fn format_env_set(&self, key: &str, value: &str) -> String {
        format!("${} = {}", self.quote(key), self.quote(value))
    }

    fn format_env_unset(&self, key: &str) -> String {
        format!(r#"del ${key}"#)
    }

    fn format_path_set(&self, paths: &[String]) -> String {
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

    fn quote(&self, value: &str) -> String {
        if value.contains(' ') || value.contains('$') || value.contains('"') || value.contains('\\')
        {
            format!("\"{}\"", value.replace("\\", "\\\\").replace("\"", "\\\""))
        } else {
            value.to_string()
        }
    }
}

impl fmt::Display for Xonsh {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "xonsh")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_env_var() {
        assert_eq!(
            Xonsh.format_env_set("PROTO_HOME", "$HOME/.proto"),
            r#"$PROTO_HOME = "$HOME/.proto""#
        );
    }

    #[test]
    fn formats_path() {
        assert_eq!(
            Xonsh.format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"$PATH = "$PROTO_HOME/shims:$PROTO_HOME/bin:$PATH""#
        );
    }

    #[test]
    fn test_quote() {
        assert_eq!(Xonsh.quote("simplevalue"), "simplevalue");
        assert_eq!(Xonsh.quote("value with spaces"), "\"value with spaces\"");
        assert_eq!(
            Xonsh.quote("value\"with\"double\"quotes"),
            "\"value\\\"with\\\"double\\\"quotes\""
        );
        assert_eq!(
            Xonsh.quote("value\\with\\backslashes"),
            "\"value\\\\with\\\\backslashes\""
        );
        assert_eq!(
            Xonsh.quote("value$with$variable"),
            "\"value$with$variable\""
        );
    }
}
