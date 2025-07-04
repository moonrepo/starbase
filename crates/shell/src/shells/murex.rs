use super::Shell;
use crate::helpers::{PATH_DELIMITER, normalize_newlines};
use crate::hooks::*;
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
    fn format(&self, statement: Statement<'_>) -> String {
        match statement {
            Statement::ModifyPath {
                paths,
                key,
                orig_key,
            } => {
                let key = key.unwrap_or("PATH");
                let value = paths.join(PATH_DELIMITER);

                match orig_key {
                    Some(orig_key) => {
                        format!(r#"$ENV.{key}="{value}{PATH_DELIMITER}$ENV.{orig_key}""#)
                    }
                    None => format!(r#"$ENV.{key}="{value}""#),
                }
            }
            Statement::SetEnv { key, value } => {
                format!("$ENV.{}={}", self.quote(key), self.quote(value))
            }
            Statement::UnsetEnv { key } => {
                format!("unset {};", self.quote(key))
            }
        }
    }

    // hook referenced from https://github.com/direnv/direnv/blob/ff451a860b31f176d252c410b43d7803ec0f8b23/internal/cmd/shell_murex.go#L12
    fn format_hook(&self, hook: Hook) -> Result<String, crate::ShellError> {
        Ok(normalize_newlines(match hook {
            Hook::OnChangeDir { command, function } => {
                format!(
                    r#"
$ENV.__ORIG_PATH="$ENV.PATH"

function {function} {{
  {command} -> source
}}

event onPrompt {function}_hook=before {{
  {function}
}}
"#
                )
            }
        }))
    }

    fn get_config_path(&self, home_dir: &Path) -> PathBuf {
        home_dir.join(".murex_profile")
    }

    fn get_env_path(&self, home_dir: &Path) -> PathBuf {
        home_dir.join(".murex_preload")
    }

    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf> {
        vec![
            home_dir.join(".murex_profile"),
            home_dir.join(".murex_preload"),
        ]
    }

    /// Quotes a string according to Murex shell quoting rules.
    /// @see <https://murex.rocks/tour.html#basic-syntax>
    fn quote(&self, value: &str) -> String {
        if value.starts_with('$') {
            return format!("\"{value}\"");
        }

        // Check for simple values that don't need quoting
        if value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return value.to_string();
        }

        // Handle brace quotes %(...)
        if value.starts_with("%(") && value.ends_with(')') {
            return value.to_string(); // Return as-is for brace quotes
        }

        // Check for values with spaces or special characters requiring double quotes
        if value.contains(' ') || value.contains('"') || value.contains('$') {
            // Escape existing backslashes and double quotes
            let escaped_value = value.replace('\\', "\\\\").replace('"', "\\\"");
            return format!("\"{escaped_value}\"");
        }

        // Default case for complex values
        value.to_string()
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
    use starbase_sandbox::assert_snapshot;

    #[test]
    fn formats_env_var() {
        assert_eq!(
            Murex.format_env_set("PROTO_HOME", "$HOME/.proto"),
            r#"$ENV.PROTO_HOME="$HOME/.proto""#
        );
    }

    #[cfg(unix)]
    #[test]
    fn formats_path_prepend() {
        assert_eq!(
            Murex.format_path_prepend(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"$ENV.PATH="$PROTO_HOME/shims:$PROTO_HOME/bin:$ENV.PATH""#
        );
    }

    #[cfg(unix)]
    #[test]
    fn formats_path_set() {
        assert_eq!(
            Murex.format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"$ENV.PATH="$PROTO_HOME/shims:$PROTO_HOME/bin""#
        );
    }

    #[cfg(windows)]
    #[test]
    fn formats_path_prepend() {
        assert_eq!(
            Murex.format_path_prepend(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"$ENV.PATH="$PROTO_HOME/shims;$PROTO_HOME/bin;$ENV.PATH""#
        );
    }

    #[cfg(windows)]
    #[test]
    fn formats_path_set() {
        assert_eq!(
            Murex.format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"$ENV.PATH="$PROTO_HOME/shims;$PROTO_HOME/bin""#
        );
    }

    #[test]
    fn formats_cd_hook() {
        let hook = Hook::OnChangeDir {
            command: "starbase hook murex".into(),
            function: "_starbase_hook".into(),
        };

        assert_snapshot!(Murex.format_hook(hook).unwrap());
    }

    #[test]
    fn test_profile_paths() {
        #[allow(deprecated)]
        let home_dir = std::env::home_dir().unwrap();

        assert_eq!(
            Murex::new().get_profile_paths(&home_dir),
            vec![
                home_dir.join(".murex_profile"),
                home_dir.join(".murex_preload"),
            ]
        );
    }

    #[test]
    fn test_murex_quoting() {
        assert_eq!(Murex.quote("value"), "value");
        assert_eq!(Murex.quote("value with spaces"), r#""value with spaces""#);
        assert_eq!(Murex.quote("$(echo hello)"), "\"$(echo hello)\"");
        assert_eq!(Murex.quote(""), "");
        assert_eq!(Murex.quote("abc123"), "abc123");
        assert_eq!(Murex.quote("%(Bob)"), "%(Bob)");
        assert_eq!(Murex.quote("%(hello world)"), "%(hello world)");
    }
}
