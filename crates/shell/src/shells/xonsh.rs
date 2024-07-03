use super::Shell;
use crate::helpers::get_config_dir;
use crate::hooks::*;
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
    fn format(&self, statement: Statement<'_>) -> String {
        match statement {
            Statement::PrependPath {
                paths,
                key,
                orig_key,
            } => {
                let key = key.unwrap_or("PATH");
                let orig_key = orig_key.unwrap_or(key);

                format!(r#"${key} = "{}:${orig_key}""#, paths.join(":"))
            }
            Statement::SetEnv { key, value } => {
                format!("${key} = {}", self.quote(value))
            }
            Statement::UnsetEnv { key } => {
                format!("del ${key}")
            }
        }
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

    /// Quotes a string according to Xonsh shell quoting rules.
    /// @see <https://xon.sh/tutorial_subproc_strings.html>
    fn quote(&self, value: &str) -> String {
        if value.is_empty() {
            return "''".to_string();
        }

        let mut quoted = String::new();
        for c in value.chars() {
            match c {
                '"' => quoted.push_str("\\\""),
                '\\' => quoted.push_str("\\\\"),
                _ => quoted.push(c),
            }
        }

        format!("\"{}\"", quoted)
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
    fn test_xonsh_quoting() {
        let xonsh = Xonsh::new();
        assert_eq!(xonsh.quote(""), "''");
        assert_eq!(xonsh.quote("simple"), "\"simple\"");
        assert_eq!(xonsh.quote("don't"), "\"don't\"");
        assert_eq!(xonsh.quote("say \"hello\""), "\"say \\\"hello\\\"\"");
        assert_eq!(xonsh.quote("price $5"), "\"price $5\"");
        assert_eq!(
            xonsh.quote("complex 'value' with \"quotes\" and \\backslashes\\"),
            "\"complex 'value' with \\\"quotes\\\" and \\\\backslashes\\\\\""
        );
    }
}
