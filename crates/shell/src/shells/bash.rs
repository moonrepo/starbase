use super::Shell;
use crate::helpers::normalize_newlines;
use crate::hooks::*;
use shell_quote::{Bash as BashQuote, Quotable, QuoteRefExt};
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
            Statement::ModifyPath {
                paths,
                key,
                orig_key,
            } => {
                let key = key.unwrap_or("PATH");
                let mut value = paths.join(":");

                if let Some(orig) = orig_key {
                    value.push_str(":$");
                    value.push_str(orig);
                }

                format!(r#"export {key}="{value}";"#)
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
            vec![home_dir.join(".bashrc"), home_dir.join(".profile")]
        }
    }

    /// Quotes a string according to Bash shell quoting rules.
    /// @see <https://www.gnu.org/software/bash/manual/bash.html#Quoting>
    fn quote<'a, T: Into<Quotable<'a>>>(&self, value: T) -> String {
        value.quoted(BashQuote)
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
    fn formats_path_prepend() {
        assert_eq!(
            Bash::new()
                .format_path_prepend(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            "export PATH=\"$PROTO_HOME/shims:$PROTO_HOME/bin:$PATH\";"
        );
    }

    #[test]
    fn formats_path_set() {
        assert_eq!(
            Bash::new().format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            "export PATH=\"$PROTO_HOME/shims:$PROTO_HOME/bin\";"
        );
    }

    #[test]
    fn formats_cd_hook() {
        let hook = Hook::OnChangeDir {
            command: "starbase hook bash".into(),
            function: "_starbase_hook".into(),
        };

        assert_snapshot!(Bash.format_hook(hook).unwrap());
    }

    #[test]
    fn test_profile_paths() {
        #[allow(deprecated)]
        let home_dir = std::env::home_dir().unwrap();

        if has_bash_profile(&home_dir) {
            assert_eq!(
                Bash::new().get_profile_paths(&home_dir),
                vec![
                    home_dir.join(".bash_profile"),
                    home_dir.join(".bashrc"),
                    home_dir.join(".profile")
                ]
            );
        } else {
            assert_eq!(
                Bash::new().get_profile_paths(&home_dir),
                vec![home_dir.join(".bashrc"), home_dir.join(".profile")]
            );
        }
    }

    #[test]
    fn test_bash_quoting() {
        let shell = Bash;
        assert_eq!(shell.quote("simple"), "simple"); // No quoting needed
        assert_eq!(shell.quote("value with spaces"), "$'value with spaces'"); // Double quotes needed
        assert_eq!(shell.quote("value\"with\"quotes"), "$'value\"with\"quotes'"); // Double quotes with escaping
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
        assert_eq!(
            shell.quote("value with \"quotes\" and $VAR"),
            "\"value with \\\"quotes\\\" and $VAR\""
        ); // Double quotes
    }
}
