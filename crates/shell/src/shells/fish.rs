use super::Shell;
use crate::helpers::get_config_dir;
use crate::hooks::Hook;
use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Fish;

impl Fish {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self
    }
}

// https://fishshell.com/docs/current/language.html#configuration
impl Shell for Fish {
    fn format_env_set(&self, key: &str, value: &str) -> String {
        format!(r#"set -gx {key} "{value}";"#)
    }

    fn format_env_unset(&self, key: &str) -> String {
        format!(r#"set -ge {key};"#)
    }

    fn format_path_set(&self, paths: &[String]) -> String {
        format!(r#"set -gx PATH "{}" $PATH;"#, paths.join(":"))
    }

    fn format_hook(&self, hook: Hook) -> Result<String, crate::ShellError> {
        Ok(hook.render_template(
            self,
            r#"
function __{prefix}_hook --on-variable PWD;
{export_env}
{export_path}
end;"#,
            "    ",
        ))
    }

    fn get_config_path(&self, home_dir: &Path) -> PathBuf {
        get_config_dir(home_dir).join("fish").join("config.fish")
    }

    fn get_env_path(&self, home_dir: &Path) -> PathBuf {
        self.get_config_path(home_dir)
    }

    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf> {
        HashSet::<PathBuf>::from_iter([
            get_config_dir(home_dir).join("fish").join("config.fish"),
            home_dir.join(".config").join("fish").join("config.fish"),
        ])
        .into_iter()
        .collect()
    }
}

impl fmt::Display for Fish {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "fish")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use starbase_sandbox::assert_snapshot;

    #[test]
    fn formats_env_var() {
        assert_eq!(
            Fish.format_env_set("PROTO_HOME", "$HOME/.proto"),
            r#"set -gx PROTO_HOME "$HOME/.proto";"#
        );
    }

    #[test]
    fn formats_path() {
        assert_eq!(
            Fish.format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"set -gx PATH "$PROTO_HOME/shims:$PROTO_HOME/bin" $PATH;"#
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

        assert_snapshot!(Fish.format_hook(hook).unwrap());
    }
}
