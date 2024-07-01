use super::Shell;
use crate::hooks::Hook;
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
    fn format_env_set(&self, key: &str, value: &str) -> String {
        format!("export {}={};", self.quote(key), self.quote(value))
    }

    fn format_env_unset(&self, key: &str) -> String {
        format!("unset {};", self.quote(key))
    }

    fn format_path_set(&self, paths: &[String]) -> String {
        format!(r#"export PATH="{}:$PATH";"#, paths.join(":"))
    }

    fn format_hook(&self, hook: Hook) -> Result<String, crate::ShellError> {
        Ok(hook.render_template(
            self,
            r#"
_{prefix}_hook() {
  local previous_exit_status=$?;
  trap -- '' SIGINT;
{export_env}
{export_path}
  trap - SIGINT;
  return $previous_exit_status;
};

if [[ ";${PROMPT_COMMAND[*]:-};" != *";_{prefix}_hook;"* ]]; then
  if [[ "$(declare -p PROMPT_COMMAND 2>&1)" == "declare -a"* ]]; then
    PROMPT_COMMAND=(_{prefix}_hook "${PROMPT_COMMAND[@]}")
  else
    PROMPT_COMMAND="_{prefix}_hook${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
  fi
fi
"#,
            "  ",
        ))
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

    /// Quotes a string according to Bash shell quoting rules.
    /// @see <https://www.gnu.org/software/bash/manual/html_node/Qu>
    fn quote(&self, value: &str) -> String {
        // No quoting needed for alphanumeric and underscore characters
        if value.is_empty() || value.chars().all(|c| c.is_alphanumeric() || c == '_') {
            value.to_string()
        } else if value
            .chars()
            .any(|c| c == '\n' || c == '\t' || c == '\\' || c == '\'')
        {
            // Use $'...' ANSI-C quoting for values containing special characters
            format!(
                "$'{}'",
                value
                    .replace('\\', "\\\\")
                    .replace('\'', "\\'")
                    .replace('\n', "\\n")
                    .replace('\t', "\\t")
            )
        } else {
            // Use double quotes for values containing special characters not handled by ANSI-C
            format!("\"{}\"", value.replace('"', "\\\""))
        }
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
    use starbase_sandbox::assert_snapshot;

    #[test]
    fn formats_env_var() {
        assert_eq!(
            Bash.format_env_set("PROTO_HOME", "$HOME/.proto"),
            "export PROTO_HOME=\"$HOME/.proto\";"
        );
    }

    #[test]
    fn formats_path() {
        assert_eq!(
            Bash.format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            "export PATH=\"$PROTO_HOME/shims:$PROTO_HOME/bin:$PATH\";"
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

        assert_snapshot!(Bash.format_hook(hook).unwrap());
    }

    #[test]
    fn test_bash_quoting() {
        let shell = Bash;
        assert_eq!(shell.quote("simple"), "simple"); // No quoting needed
        assert_eq!(shell.quote("value with spaces"), "\"value with spaces\""); // Double quotes needed
        assert_eq!(
            shell.quote("value\"with\"quotes"),
            "\"value\\\"with\\\"quotes\""
        ); // Double quotes with escaping
        assert_eq!(
            shell.quote("value\nwith\nnewlines"),
            "$'value\\nwith\\nnewlines'"
        ); // ANSI-C quoting for newlines
        assert_eq!(shell.quote("value\twith\ttabs"), "$'value\\twith\\ttabs'"); // ANSI-C quoting for tabs
        assert_eq!(
            shell.quote("value\\with\\backslashes"),
            "$'value\\\\with\\\\backslashes'"
        ); // ANSI-C quoting for backslashes
        assert_eq!(shell.quote("value'with'quotes"), "$'value\\'with\\'quotes'");
        // ANSI-C quoting for single quotes
    }
}
