use super::Shell;
use crate::hooks::OnCdHook;
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
        format!(r#"export {key}="{value}";"#)
    }

    fn format_env_unset(&self, key: &str) -> String {
        format!(r#"unset {key};"#)
    }

    fn format_path_set(&self, paths: &[String]) -> String {
        if paths.is_empty() {
            "".into()
        } else {
            format!(r#"export PATH="{}:$PATH";"#, paths.join(":"))
        }
    }

    fn format_on_cd_hook(&self, hook: OnCdHook) -> Result<String, crate::ShellError> {
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
        let mut hook = OnCdHook {
            prefix: "starbase".into(),
            ..OnCdHook::default()
        };

        assert_snapshot!(Bash.format_on_cd_hook(hook.clone()).unwrap());

        hook.paths
            .extend(["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]);
        hook.env.extend([
            ("PROTO_HOME".into(), Some("$HOME/.proto".into())),
            ("PROTO_ROOT".into(), None),
        ]);

        assert_snapshot!(Bash.format_on_cd_hook(hook).unwrap());
    }
}
