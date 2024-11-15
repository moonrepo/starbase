use super::Shell;
use crate::helpers::{get_config_dir, normalize_newlines};
use crate::hooks::*;
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
    fn format(&self, statement: Statement<'_>) -> String {
        match statement {
            Statement::PrependPath {
                paths,
                key,
                orig_key,
            } => {
                let key = key.unwrap_or("PATH");
                let orig_key = orig_key.unwrap_or(key);

                format!(
                    r#"set -gx {key} {} ${orig_key};"#,
                    paths
                        .iter()
                        .map(|p| self.quote(p))
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            }
            Statement::SetEnv { key, value } => {
                format!("set -gx {} {};", key, self.quote(value))
            }
            Statement::UnsetEnv { key } => {
                format!("set -ge {key};")
            }
        }
    }

    fn format_hook(&self, hook: Hook) -> Result<String, crate::ShellError> {
        Ok(normalize_newlines(match hook {
            Hook::OnChangeDir { command, function } => {
                format!(
                    r#"
set -gx __ORIG_PATH $PATH

function {function} --on-variable PWD;
  {command} | source
end;
"#
                )
            }
        }))
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

    /// Quotes a string according to Fish shell quoting rules.
    /// @see <https://fishshell.com/docs/current/language.html#quotes>
    fn quote(&self, value: &str) -> String {
        if value.is_empty() {
            return "''".to_string();
        }

        // Characters that need to be escaped in double quotes
        let escape_chars: &[(char, &str)] = &[
            ('\\', "\\\\"),
            ('\n', "\\n"),
            ('\t', "\\t"),
            ('\x07', "\\a"),
            ('\x08', "\\b"),
            ('\x1b', "\\e"),
            ('\x0c', "\\f"),
            ('\x0a', "\\n"),
            ('\x0d', "\\r"),
            ('\x0b', "\\v"),
            ('*', "\\*"),
            ('?', "\\?"),
            ('~', "\\~"),
            ('#', "\\#"),
            ('(', "\\("),
            (')', "\\)"),
            ('{', "\\{"),
            ('}', "\\}"),
            ('[', "\\["),
            (']', "\\]"),
            ('<', "\\<"),
            ('>', "\\>"),
            ('^', "\\^"),
            ('&', "\\&"),
            ('|', "\\|"),
            (';', "\\;"),
            ('"', "\\\""),
            // ('$', "\\$"),
        ];

        let mut quoted = value.to_string();
        for &(char, escape) in escape_chars.iter() {
            quoted = quoted.replace(char, escape);
        }

        if quoted.contains(' ') {
            format!("'{}'", quoted.replace('\'', "''"))
        } else {
            format!(r#""{}""#, quoted)
        }
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
            r#"set -gx PATH "$PROTO_HOME/shims" "$PROTO_HOME/bin" $PATH;"#
        );
    }

    #[test]
    fn formats_cd_hook() {
        let hook = Hook::OnChangeDir {
            command: "starbase hook fish".into(),
            function: "_starbase_hook".into(),
        };

        assert_snapshot!(Fish.format_hook(hook).unwrap());
    }

    #[test]
    fn test_fish_quoting() {
        assert_eq!(Fish.quote("\n"), r#""\n""#);
        assert_eq!(Fish.quote("\t"), r#""\t""#);
        assert_eq!(Fish.quote("\x07"), r#""\a""#);
        assert_eq!(Fish.quote("\x08"), r#""\b""#);
        assert_eq!(Fish.quote("\x1b"), r#""\e""#);
        assert_eq!(Fish.quote("\x0c"), r#""\f""#);
        assert_eq!(Fish.quote("\r"), r#""\r""#);
        assert_eq!(Fish.quote("\x0a"), r#""\n""#);
        assert_eq!(Fish.quote("\x0b"), r#""\v""#);
        assert_eq!(Fish.quote("*"), r#""\*""#);
        assert_eq!(Fish.quote("?"), r#""\?""#);
        assert_eq!(Fish.quote("~"), r#""\~""#);
        assert_eq!(Fish.quote("#"), r#""\#""#);
        assert_eq!(Fish.quote("("), r#""\(""#);
        assert_eq!(Fish.quote(")"), r#""\)""#);
        assert_eq!(Fish.quote("{"), r#""\{""#);
        assert_eq!(Fish.quote("}"), r#""\}""#);
        assert_eq!(Fish.quote("["), r#""\[""#);
        assert_eq!(Fish.quote("]"), r#""\]""#);
        assert_eq!(Fish.quote("<"), r#""\<""#);
        assert_eq!(Fish.quote(">"), r#""\>""#);
        assert_eq!(Fish.quote("^"), r#""\^""#);
        assert_eq!(Fish.quote("&"), r#""\&""#);
        assert_eq!(Fish.quote("|"), r#""\|""#);
        assert_eq!(Fish.quote(";"), r#""\;""#);
        assert_eq!(Fish.quote("\""), r#""\"""#);
        // assert_eq!(Fish.quote("$"), r#""\$""#);
        // assert_eq!(Fish.quote("$variable"), r#""\$variable""#);
        assert_eq!(Fish.quote("value with spaces"), "'value with spaces'");
    }
}
