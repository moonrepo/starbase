use super::Shell;
use crate::helpers::normalize_newlines;
use crate::hooks::*;
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

fn has_bash_profile(home_dir: &Path) -> bool {
    home_dir.join(".bash_profile").exists()
}

fn profile_for_bash(home_dir: &Path) -> PathBuf {
    // https://github.com/moonrepo/starbase/issues/99
    // Ubuntu doesn't have .bash_profile. It uses .profile instead.
    // If .bash_profile is newly created, .profile will be no longer loaded.
    if has_bash_profile(home_dir) {
        home_dir.join(".bash_profile")
    } else {
        home_dir.join(".profile")
    }
}

// https://www.baeldung.com/linux/bashrc-vs-bash-profile-vs-profile
impl Shell for Bash {
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

    // https://mywiki.wooledge.org/SignalTrap
    fn format_hook(&self, hook: Hook) -> Result<String, crate::ShellError> {
        Ok(normalize_newlines(match hook {
            Hook::OnChangeDir { command, function } => {
                format!(
                    r#"
export __ORIG_PATH="$PATH"

{function}() {{
  local previous_exit_status=$?;
  trap '' SIGINT;
  output=$({command})
  if [ -n "$output" ]; then
    eval "$output";
  fi
  trap - SIGINT;
  return $previous_exit_status;
}};

if [[ ";${{PROMPT_COMMAND[*]:-}};" != *";{function};"* ]]; then
  if [[ "$(declare -p PROMPT_COMMAND 2>&1)" == "declare -a"* ]]; then
    PROMPT_COMMAND=({function} "${{PROMPT_COMMAND[@]}}")
  else
    PROMPT_COMMAND="{function}${{PROMPT_COMMAND:+;$PROMPT_COMMAND}}"
  fi
fi
"#
                )
            }
        }))
    }

    fn get_config_path(&self, home_dir: &Path) -> PathBuf {
        profile_for_bash(home_dir)
    }

    fn get_env_path(&self, home_dir: &Path) -> PathBuf {
        profile_for_bash(home_dir)
    }

    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf> {
        if has_bash_profile(home_dir) {
            vec![
                home_dir.join(".bash_profile"),
                home_dir.join(".bashrc"),
                home_dir.join(".profile"),
            ]
        } else {
            // Default .profile calls .bashrc in Ubuntu
            vec![home_dir.join(".profile"), home_dir.join(".bashrc")]
        }
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
            command: "starbase hook bash".into(),
            function: "starbase".into(),
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
