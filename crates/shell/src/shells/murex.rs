use super::Shell;
use crate::hooks::Hook;
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
    fn format_env_set(&self, key: &str, value: &str) -> String {
        format!("$ENV.{}={}", self.quote(key), self.quote(value))
    }

    fn format_env_unset(&self, key: &str) -> String {
        format!("unset {};", self.quote(key))
    }

    fn format_path_set(&self, paths: &[String]) -> String {
        format!(r#"$ENV.PATH="{}:$ENV.PATH""#, paths.join(":"))
    }

    // hook referenced from https://github.com/direnv/direnv/blob/ff451a860b31f176d252c410b43d7803ec0f8b23/internal/cmd/shell_murex.go#L12
    fn format_hook(&self, hook: Hook) -> Result<String, crate::ShellError> {
        Ok(hook.render_template(
            self,
            r#"
event: onPrompt {prefix}_hook=before {
{export_env}
{export_path}
}"#,
            "  ",
        ))
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
            home_dir.join(".murex_profile"),
        ]
    }

    fn quote(&self, value: &str) -> String {
        if value.starts_with('$') {
            return format!("\"{}\"", value);
        }
        // Check for simple values that don't need quoting
        if value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            return value.to_string();
        }
    
        // Handle brace quotes %(...)
        if value.starts_with("%(") && value.ends_with(")") {
            return value.to_string(); // Return as-is for brace quotes
        }
    
        // Check for values with spaces or special characters requiring double quotes
        if value.contains(' ') || value.contains('"') || value.contains('$') {
            // Escape existing backslashes and double quotes
            let escaped_value = value.replace("\\", "\\\\").replace("\"", "\\\"");
            return format!("\"{}\"", escaped_value);
        }
    
        // Default case for complex values
        format!("{}", value)
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

    #[test]
    fn formats_path() {
        assert_eq!(
            Murex.format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"$ENV.PATH="$PROTO_HOME/shims:$PROTO_HOME/bin:$ENV.PATH""#
        );
    }

    #[test]
    fn formats_cd_hook() {
        let hook = Hook::OnChangeDir {
            env: vec![
                ("PROTO_HOME".into(), Some("$HOME/.proto".into())),
                ("PROTO_ROOT".into(), None),
            ],
            paths: vec!["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()],
            prefix: "starbase".into(),
        };

        assert_snapshot!(Murex.format_hook(hook).unwrap());
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
