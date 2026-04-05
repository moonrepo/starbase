use super::{Sh, Shell};
use crate::hooks::*;
use crate::quoter::*;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Ash {
    inner: Sh,
}

impl Ash {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self { inner: Sh::new() }
    }
}

// https://github.com/ash-shell/ash
impl Shell for Ash {
    fn create_quoter<'a>(&self, data: Quotable<'a>) -> Quoter<'a> {
        self.inner.create_quoter(data)
    }

    fn format(&self, statement: Statement<'_>) -> String {
        self.inner.format(statement)
    }

    fn get_config_path(&self, home_dir: &Path) -> PathBuf {
        home_dir.join(".ashrc")
    }

    fn get_env_path(&self, home_dir: &Path) -> PathBuf {
        home_dir.join(".ashrc")
    }

    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf> {
        vec![home_dir.join(".ashrc"), home_dir.join(".profile")]
    }
}

impl fmt::Display for Ash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ash")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_env_var() {
        assert_eq!(
            Ash::new().format_env_set("PROTO_HOME", "$HOME/.proto"),
            "export PROTO_HOME=\"$HOME/.proto\";"
        );
    }

    #[test]
    fn formats_path_prepend() {
        assert_eq!(
            Ash::new().format_path_prepend(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            "export PATH=\"$PROTO_HOME/shims:$PROTO_HOME/bin:$PATH\";"
        );
    }

    #[test]
    fn formats_path_set() {
        assert_eq!(
            Ash::new().format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            "export PATH=\"$PROTO_HOME/shims:$PROTO_HOME/bin\";"
        );
    }

    #[test]
    fn test_profile_paths() {
        #[allow(deprecated)]
        let home_dir = std::env::home_dir().unwrap();

        assert_eq!(
            Ash::new().get_profile_paths(&home_dir),
            vec![home_dir.join(".ashrc"), home_dir.join(".profile")]
        );
    }

    #[test]
    fn formats_alias_set() {
        assert_eq!(
            Ash::new().format_alias_set("ll", "ls -la"),
            "alias ll=ls' -la';"
        );
    }

    #[test]
    fn formats_alias_unset() {
        assert_eq!(Ash::new().format_alias_unset("ll"), "unalias ll;");
    }

    #[test]
    fn test_ash_quoting() {
        let shell = Ash::new();
        assert_eq!(shell.quote("simple"), "simple"); // No quoting needed
        assert_eq!(shell.quote("value with spaces"), "value' with spaces'"); // POSIX-style single quotes
        assert_eq!(
            shell.quote("value\"with\"quotes"),
            "\"value\\\"with\\\"quotes\""
        ); // Double quotes with escaping
        assert_eq!(
            shell.quote("value\nwith\nnewlines"),
            "\"value\\nwith\\nnewlines\""
        ); // Double quotes with escaped newlines
        assert_eq!(shell.quote("value\twith\ttabs"), "\"value\\twith\\ttabs\""); // Double quotes with escaped tabs
        assert_eq!(
            shell.quote("value\\with\\backslashes"),
            "\"value\\\\with\\\\backslashes\""
        ); // Double quotes with escaped backslashes
        assert_eq!(shell.quote("value'with'quotes"), "value\\'with\\'quotes");
        // ANSI-C quoting for single quotes
    }
}
