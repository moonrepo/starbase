use super::Shell;
use crate::utils::escape::escapeBash;
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
        // format!("export {}={};", self.quote(key), self.quote(value))
        format!("export {}={};", escapeBash(key), escapeBash(value))
    }

    fn format_env_unset(&self, key: &str) -> String {
        // format!("unset {key};", key=self.quote(key))
        format!("unset {key};", key=escapeBash(key))
    }    

    fn format_path_set(&self, paths: &[String]) -> String {
        // let escaped_paths: Vec<String> = paths.iter().map(|p| self.quote(p)).collect();
        // format!(r#"export PATH="{}:$PATH";"#, escaped_paths.join(":"))
        let escaped_paths: Vec<String> = paths.iter().map(|p| escapeBash(p)).collect();
        format!(r#"export PATH="{}:$PATH";"#, escaped_paths.join(":"))
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

    // TODO: commented out to get some basic tests work first
    // fn quote(&self, value: &str) -> String {
    //     escapeBash(value)
    // }
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
            r#"export PROTO_HOME="$HOME/.proto";"#
        );
    }

    #[test]
    fn formats_path() {
        assert_eq!(
            Bash.format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"export PATH="$PROTO_HOME/shims:$PROTO_HOME/bin:$PATH";"#
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
    fn test_escape_plain_string() {
        // let bash_shell = Bash::new();
        assert_snapshot!(escapeBash("foobar"));
    }

    #[test]
    fn test_escape_string_with_spaces() {
        // let bash_shell = Bash::new();
        assert_snapshot!(escapeBash("foo bar"));
    }

    #[test]
    fn test_escape_string_with_single_quotes() {
        // let bash_shell = Bash::new();
        assert_snapshot!(escapeBash("don't"));
    }

    #[test]
    fn test_escape_string_with_special_characters() {
        // let bash_shell = Bash::new();
        assert_snapshot!(escapeBash("$100@*&"));
    }

    #[test]
    fn test_escape_string_with_backslashes() {
        // let bash_shell = Bash::new();
        assert_snapshot!(escapeBash("a\\b"));
    }

    #[test]
    fn test_escape_empty_string() {
        // let bash_shell = Bash::new();
        assert_snapshot!(escapeBash(""));
    }
}
