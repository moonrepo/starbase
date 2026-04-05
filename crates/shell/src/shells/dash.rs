use super::{Sh, Shell};
use crate::hooks::*;
use crate::quoter::*;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Dash {
    inner: Sh,
}

impl Dash {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self { inner: Sh::new() }
    }
}

// https://github.com/ash-shell/ash
impl Shell for Dash {
    fn create_quoter<'a>(&self, data: Quotable<'a>) -> Quoter<'a> {
        self.inner.create_quoter(data)
    }

    fn format(&self, statement: Statement<'_>) -> String {
        self.inner.format(statement)
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

impl fmt::Display for Dash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "dash")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_env_var() {
        assert_eq!(
            Dash::new().format_env_set("PROTO_HOME", "$HOME/.proto"),
            "export PROTO_HOME=\"$HOME/.proto\";"
        );
    }

    #[test]
    fn formats_path_prepend() {
        assert_eq!(
            Dash::new()
                .format_path_prepend(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            "export PATH=\"$PROTO_HOME/shims:$PROTO_HOME/bin:$PATH\";"
        );
    }

    #[test]
    fn formats_path_set() {
        assert_eq!(
            Dash::new().format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            "export PATH=\"$PROTO_HOME/shims:$PROTO_HOME/bin\";"
        );
    }

    #[test]
    fn test_profile_paths() {
        #[allow(deprecated)]
        let home_dir = std::env::home_dir().unwrap();

        assert_eq!(
            Dash::new().get_profile_paths(&home_dir),
            vec![home_dir.join(".profile")]
        );
    }

    #[test]
    fn formats_alias_set() {
        assert_eq!(
            Dash::new().format_alias_set("ll", "ls -la"),
            "alias ll=ls' -la';"
        );
    }

    #[test]
    fn formats_alias_unset() {
        assert_eq!(Dash::new().format_alias_unset("ll"), "unalias ll;");
    }

    #[test]
    fn test_dash_quoting() {
        let sh = Sh::new();
        assert_eq!(sh.quote(""), "''");
        assert_eq!(sh.quote("simple"), "simple");
        assert_eq!(sh.quote("say \"hello\""), "\"say \\\"hello\\\"\"");
        assert_eq!(sh.quote("price $5"), "\"price $5\"");
        assert_eq!(
            sh.quote("complex 'value' with \"quotes\" and \\backslashes\\"),
            "\"complex 'value' with \\\"quotes\\\" and \\\\backslashes\\\\\""
        );
    }
}
