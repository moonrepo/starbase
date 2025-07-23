use super::Shell;
use crate::helpers::{ProfileSet, get_config_dir, normalize_newlines};
use crate::hooks::*;
use crate::quoter::*;
use shell_quote::{Fish as FishQuote, Quotable, QuoteRefExt};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
    fn create_quoter<'a>(&self, data: Quotable<'a>) -> Quoter<'a> {
        Quoter::new(
            data,
            QuoterOptions {
                on_quote: Arc::new(|data| data.quoted(FishQuote)),
                ..Default::default()
            },
        )
    }

    fn format(&self, statement: Statement<'_>) -> String {
        match statement {
            Statement::ModifyPath {
                paths,
                key,
                orig_key,
            } => {
                let key = key.unwrap_or("PATH");
                let value = paths
                    .iter()
                    .map(|p| format!(r#""{p}""#))
                    .collect::<Vec<_>>()
                    .join(" ");

                match orig_key {
                    Some(orig_key) => format!("set -gx {key} {value} ${orig_key};"),
                    None => format!("set -gx {key} {value};"),
                }
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
        ProfileSet::default()
            .insert(get_config_dir(home_dir).join("fish").join("config.fish"), 1)
            .insert(home_dir.join(".config").join("fish").join("config.fish"), 2)
            .into_list()
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
    fn formats_path_prepend() {
        assert_eq!(
            Fish.format_path_prepend(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"set -gx PATH "$PROTO_HOME/shims" "$PROTO_HOME/bin" $PATH;"#
        );
    }

    #[test]
    fn formats_path_set() {
        assert_eq!(
            Fish.format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"set -gx PATH "$PROTO_HOME/shims" "$PROTO_HOME/bin";"#
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
    fn test_profile_paths() {
        #[allow(deprecated)]
        let home_dir = std::env::home_dir().unwrap();

        assert_eq!(
            Fish::new().get_profile_paths(&home_dir),
            vec![home_dir.join(".config").join("fish").join("config.fish")]
        );
    }

    #[test]
    fn test_fish_quoting() {
        // assert_eq!(Fish.quote("\n"), r#"\n"#);
        // assert_eq!(Fish.quote("\t"), r#"\t"#);
        // assert_eq!(Fish.quote("\x07"), r#"\a"#);
        // assert_eq!(Fish.quote("\x08"), r#"\b"#);
        // assert_eq!(Fish.quote("\x1b"), r#"\e"#);
        // assert_eq!(Fish.quote("\x0c"), r#"\f"#);
        // assert_eq!(Fish.quote("\r"), r#"\r"#);
        // assert_eq!(Fish.quote("\x0a"), r#"\n"#);
        // assert_eq!(Fish.quote("\x0b"), r#"\v"#);
        // assert_eq!(Fish.quote("*"), r#""\*""#);
        // assert_eq!(Fish.quote("?"), r#""\?""#);
        // assert_eq!(Fish.quote("~"), r#""\~""#);
        // assert_eq!(Fish.quote("#"), r#""\#""#);
        // assert_eq!(Fish.quote("("), r#""\(""#);
        // assert_eq!(Fish.quote(")"), r#""\)""#);
        // assert_eq!(Fish.quote("{"), r#""\{""#);
        // assert_eq!(Fish.quote("}"), r#""\}""#);
        // assert_eq!(Fish.quote("["), r#""\[""#);
        // assert_eq!(Fish.quote("]"), r#""\]""#);
        // assert_eq!(Fish.quote("<"), r#""\<""#);
        // assert_eq!(Fish.quote(">"), r#""\>""#);
        // assert_eq!(Fish.quote("^"), r#""\^""#);
        // assert_eq!(Fish.quote("&"), r#""\&""#);
        // assert_eq!(Fish.quote("|"), r#""\|""#);
        // assert_eq!(Fish.quote(";"), r#""\;""#);
        // assert_eq!(Fish.quote("\""), r#""\"""#);
        assert_eq!(Fish.quote("$"), "'$'");
        assert_eq!(Fish.quote("$variable"), "\"$variable\"");
        assert_eq!(Fish.quote("value with spaces"), "value' with spaces'");
    }
}
