use super::Shell;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Bash;

impl Bash {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self
    }
}

// https://www.baeldung.com/linux/bashrc-vs-bash-profile-vs-profile
impl Shell for Bash {
    fn format_env_export(&self, key: &str, value: &str) -> String {
        format!(r#"export {key}="{value}""#)
    }

    fn format_path_export(&self, paths: &[String]) -> String {
        format!(r#"export PATH="{}:$PATH""#, paths.join(":"))
    }

    fn get_config_path(&self, home_dir: &Path) -> PathBuf {
        home_dir.join(".bash_profile")
    }

    fn get_env_path(&self, home_dir: &Path) -> PathBuf {
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

impl fmt::Display for Bash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bash")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_env_var() {
        assert_eq!(
            Bash.format_env_export("PROTO_HOME", "$HOME/.proto"),
            r#"export PROTO_HOME="$HOME/.proto""#
        );
    }

    #[test]
    fn formats_path() {
        assert_eq!(
            Bash.format_path_export(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"export PATH="$PROTO_HOME/shims:$PROTO_HOME/bin:$PATH""#
        );
    }
}
