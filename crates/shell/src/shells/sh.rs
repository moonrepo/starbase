use super::Shell;
use crate::hooks::*;
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
    fn format(&self, statement: Statement<'_>) -> String {
        match statement {
            Statement::PrependPath {
                paths,
                key,
                orig_key,
            } => {
                let key = key.unwrap_or("PATH");
                let orig_key = orig_key.unwrap_or(key);

                format!(r#"export {key}="{}:${orig_key}";"#, paths.join(":"))
            }
            Statement::SetEnv { key, value } => {
                format!("export {}={};", self.quote(key), self.quote(value))
            }
            Statement::UnsetEnv { key } => {
                format!("unset {};", self.quote(key))
            }
        }
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

    /// Quotes a string according to shell quoting rules.
    /// @see <https://rg1-teaching.mpi-inf.mpg.de/unixffb-ss98/quoting-guide.html>
    fn quote(&self, value: &str) -> String {
        if value.is_empty() {
            return "''".to_string();
        }

        // Check if we need double quotes
        if value.contains('\'')
            || value.contains('\"')
            || value.contains('`')
            || value.contains(' ')
        {
            // Use double quotes and escape necessary characters
            let mut quoted = String::from("\"");

            for c in value.chars() {
                match c {
                    '"' | '\\' | '$' | '`' => {
                        quoted.push('\\');
                        quoted.push(c);
                    }
                    _ => {
                        quoted.push(c);
                    }
                }
            }

            quoted.push('"');
            quoted
        } else {
            // Otherwise, use single quotes for literals and variables
            // Check if it starts with a variable
            if value.starts_with('$') {
                format!("\"{}\"", value)
            } else {
                value.to_string()
            }
        }
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

    #[test]
    fn test_sh_quoting() {
        let sh = Sh::new();
        assert_eq!(sh.quote(""), "''");
        assert_eq!(sh.quote("simple"), "simple");
        assert_eq!(sh.quote("say \"hello\""), "\"say \\\"hello\\\"\"");
        assert_eq!(sh.quote("price $5"), "\"price \\$5\"");
        assert_eq!(
            sh.quote("complex 'value' with \"quotes\" and \\backslashes\\"),
            "\"complex 'value' with \\\"quotes\\\" and \\\\backslashes\\\\\""
        );
    }
}
