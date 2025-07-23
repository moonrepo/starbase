use shell_quote::Quotable;

use super::{Bash, Shell};
use crate::helpers::{is_absolute_dir, normalize_newlines};
use crate::hooks::*;
use std::env;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Zsh {
    inner: Bash,
    pub dir: Option<PathBuf>,
}

impl Zsh {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            inner: Bash::new(),
            dir: env::var_os("ZDOTDIR").and_then(is_absolute_dir),
        }
    }
}

// https://zsh.sourceforge.io/Intro/intro_3.html
// https://zsh.sourceforge.io/Doc/Release/Files.html#Files
impl Shell for Zsh {
    fn format(&self, statement: Statement<'_>) -> String {
        self.inner.format(statement)
    }

    fn format_hook(&self, hook: Hook) -> Result<String, crate::ShellError> {
        Ok(normalize_newlines(match hook {
            Hook::OnChangeDir { command, function } => {
                format!(
                    r#"
export __ORIG_PATH="$PATH"

{function}() {{
  trap '' SIGINT
  output=$({command})
  if [ -n "$output" ]; then
    eval "$output";
  fi
  trap - SIGINT
}}

typeset -ag chpwd_functions
if (( ! ${{chpwd_functions[(I){function}]}} )); then
  chpwd_functions=({function} $chpwd_functions)
fi
"#
                )
            }
        }))
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
            zdot_dir.join(".zshrc"),
            zdot_dir.join(".zprofile"),
            zdot_dir.join(".zshenv"),
        ]
    }

    fn quote<'a, T: Into<Quotable<'a>>>(&self, value: T) -> String {
        self.inner.quote(value)
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
            Zsh::new().format_env_set("PROTO_HOME", "$HOME/.proto"),
            r#"export PROTO_HOME="$HOME/.proto";"#
        );
    }

    #[test]
    fn formats_path_prepend() {
        assert_eq!(
            Zsh::new().format_path_prepend(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"export PATH="$PROTO_HOME/shims:$PROTO_HOME/bin:$PATH";"#
        );
    }

    #[test]
    fn formats_path_set() {
        assert_eq!(
            Zsh::new().format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"export PATH="$PROTO_HOME/shims:$PROTO_HOME/bin";"#
        );
    }

    #[test]
    fn formats_cd_hook() {
        let hook = Hook::OnChangeDir {
            command: "starbase hook zsh".into(),
            function: "_starbase_hook".into(),
        };

        assert_snapshot!(Zsh::new().format_hook(hook).unwrap());
    }

    #[test]
    fn test_profile_paths() {
        #[allow(deprecated)]
        let home_dir = std::env::home_dir().unwrap();

        assert_eq!(
            Zsh::new().get_profile_paths(&home_dir),
            vec![
                home_dir.join(".zshrc"),
                home_dir.join(".zprofile"),
                home_dir.join(".zshenv"),
            ]
        );
    }

    #[test]
    fn test_zsh_quoting() {
        let zsh = Zsh::new();
        assert_eq!(zsh.quote(""), "''");
        assert_eq!(zsh.quote("simple"), "simple");
        assert_eq!(zsh.quote("don't"), "$'don\\'t'");
        assert_eq!(zsh.quote("say \"hello\""), "$'say \"hello\"'");
        assert_eq!(
            zsh.quote("complex 'value' with \"quotes\" and \\backslashes\\"),
            "$'complex \\'value\\' with \"quotes\" and \\\\backslashes\\\\'"
        );
    }
}
