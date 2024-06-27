use super::Shell;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Sh;

impl Sh {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self
    }
}

impl Shell for Sh {
    fn format_env_set(&self, key: &str, value: &str) -> String {
        format!(r#"export {key}="{value}";"#)
    }

    fn format_env_unset(&self, key: &str) -> String {
        format!(r#"unset {key};"#)
    }

    fn format_path_set(&self, paths: &[String]) -> String {
        format!(r#"export PATH="{}:$PATH";"#, paths.join(":"))
    }

    fn get_config_path(&self, home_dir: &Path) -> PathBuf {
        home_dir.join(".profile")
    }

    fn get_env_path(&self, home_dir: &Path) -> PathBuf {
        home_dir.join(".profile")
    }

    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf> {
        vec![home_dir.join(".profile")]
    }
}

impl fmt::Display for Sh {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "sh")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_env_var() {
        assert_eq!(
            Sh.format_env_set("PROTO_HOME", "$HOME/.proto"),
            r#"export PROTO_HOME="$HOME/.proto";"#
        );
    }

    #[test]
    fn formats_path() {
        assert_eq!(
            Sh.format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"export PATH="$PROTO_HOME/shims:$PROTO_HOME/bin:$PATH";"#
        );
    }
}
