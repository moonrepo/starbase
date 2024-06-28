use super::Shell;
use crate::helpers::is_absolute_dir;
use crate::hooks::Hook;
use std::env;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default)]
pub struct Zsh {
    pub dir: Option<PathBuf>,
}

impl Zsh {
    pub fn new() -> Self {
        Self {
            dir: env::var_os("ZDOTDIR").and_then(is_absolute_dir),
        }
    }
}

// https://zsh.sourceforge.io/Intro/intro_3.html
// https://zsh.sourceforge.io/Doc/Release/Files.html#Files
impl Shell for Zsh {
    fn format_env_set(&self, key: &str, value: &str) -> String {
        format!(r#"export {key}="{value}";"#)
    }

    fn format_env_unset(&self, key: &str) -> String {
        format!(r#"unset {key};"#)
    }

    fn format_path_set(&self, paths: &[String]) -> String {
        format!(r#"export PATH="{}:$PATH";"#, paths.join(":"))
    }

    fn format_hook(&self, hook: Hook) -> Result<String, crate::ShellError> {
        Ok(hook.render_template(
            self,
            r#"
_{prefix}_hook() {
  trap -- '' SIGINT
{export_env}
{export_path}
  trap - SIGINT
}
typeset -ag precmd_functions
if (( ! ${precmd_functions[(I)_{prefix}_hook]} )); then
  precmd_functions=(_{prefix}_hook $precmd_functions)
fi
typeset -ag chpwd_functions
if (( ! ${chpwd_functions[(I)_{prefix}_hook]} )); then
  chpwd_functions=(_{prefix}_hook $chpwd_functions)
fi
"#,
            "  ",
        ))
    }

    fn get_config_path(&self, home_dir: &Path) -> PathBuf {
        self.dir.as_deref().unwrap_or(home_dir).join(".zshrc")
    }

    fn get_env_path(&self, home_dir: &Path) -> PathBuf {
        self.dir.as_deref().unwrap_or(home_dir).join(".zshenv")
    }

    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf> {
        let zdot_dir = self.dir.as_deref().unwrap_or(home_dir);

        vec![
            zdot_dir.join(".zshenv"),
            zdot_dir.join(".zprofile"),
            zdot_dir.join(".zshrc"),
        ]
    }
}

impl fmt::Display for Zsh {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "zsh")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use starbase_sandbox::assert_snapshot;

    #[test]
    fn formats_env_var() {
        assert_eq!(
            Zsh::default().format_env_set("PROTO_HOME", "$HOME/.proto"),
            r#"export PROTO_HOME="$HOME/.proto";"#
        );
    }

    #[test]
    fn formats_path() {
        assert_eq!(
            Zsh::default().format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
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

        assert_snapshot!(Zsh::default().format_hook(hook).unwrap());
    }
}
