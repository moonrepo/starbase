use super::Shell;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Murex;

impl Murex {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self
    }
}

impl Shell for Murex {
    fn format_env_export(&self, key: &str, value: &str) -> String {
        format!(r#"$ENV.{key}="{value}""#)
    }

    fn format_path_export(&self, paths: &[String]) -> String {
        format!(r#"$ENV.PATH="{}:$ENV.PATH""#, paths.join(":"))
    }

    fn get_config_path(&self, home_dir: &Path) -> PathBuf {
        home_dir.join(".murex_profile")
    }

    fn get_env_path(&self, home_dir: &Path) -> PathBuf {
        home_dir.join(".murex_preload")
    }

    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf> {
        vec![
            home_dir.join(".murex_preload"),
            home_dir.join(".murex_modules/"),
            home_dir.join(".murex_profile"),
        ]
    }
}

impl fmt::Display for Murex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "murex")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_env_var() {
        assert_eq!(
            Murex.format_env_export("PROTO_HOME", "$HOME/.proto"),
            r#"$ENV.PROTO_HOME="$HOME/.proto""#
        );
    }

    #[test]
    fn formats_path() {
        assert_eq!(
            Murex.format_path_export(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"$ENV.PATH="$PROTO_HOME/shims:$PROTO_HOME/bin:$ENV.PATH""#
        );
    }
}
