use super::Shell;
use crate::helpers::{is_absolute_dir, normalize_newlines};
use crate::hooks::*;
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
            zdot_dir.join(".zshenv"),
            zdot_dir.join(".zprofile"),
            zdot_dir.join(".zshrc"),
        ]
    }

    /// Quotes a string according to Zsh shell quoting rules.
    /// @see <https://info2html.sourceforge.net/cgi-bin/info2html-demo/info2html?(zsh)Quoting>
    fn quote(&self, value: &str) -> String {
        if value.is_empty() {
            return "''".to_string();
        }

        let mut quoted = String::new();
        let mut is_quoted = false;

        for (i, c) in value.chars().enumerate() {
            match c {
                '\\' | '\'' | '"' => {
                    if i == 0 && c == '$' {
                        quoted.push('$');
                    }
                    quoted.push('\\');
                    quoted.push(c);
                }
                '$' => {
                    if i == 0 {
                        quoted.push_str("\"$");
                        is_quoted = true;
                    } else {
                        quoted.push('$');
                    }
                }
                _ => {
                    quoted.push(c);
                }
            }
        }

        if is_quoted {
            quoted.push('"');
        }

        quoted
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
            command: "starbase hook zsh".into(),
            function: "starbase".into(),
        };

        assert_snapshot!(Zsh::default().format_hook(hook).unwrap());
    }

    #[test]
    fn test_zsh_quoting() {
        let zsh = Zsh::new();
        assert_eq!(zsh.quote(""), "''");
        assert_eq!(zsh.quote("simple"), "simple");
        assert_eq!(zsh.quote("don't"), "don\\'t");
        assert_eq!(zsh.quote("say \"hello\""), "say \\\"hello\\\"");
        assert_eq!(
            zsh.quote("complex 'value' with \"quotes\" and \\backslashes\\"),
            "complex \\'value\\' with \\\"quotes\\\" and \\\\backslashes\\\\"
        );
    }
}
