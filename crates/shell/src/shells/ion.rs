use super::Shell;
use crate::helpers::{ProfileSet, get_config_dir};
use crate::hooks::*;
use crate::quoter::*;
use shell_quote::Quotable;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub struct Ion;

impl Ion {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self
    }

    /// Quotes a string according to Ion shell quoting rules.
    /// @see <https://doc.redox-os.org/ion-manual/general.html>
    fn do_quote(value: String) -> String {
        if value.starts_with('$') {
            // Variables expanded in double quotes
            format!("\"{value}\"")
        } else if value.contains('{') || value.contains('}') {
            // Single quotes to prevent brace expansion
            format!("'{value}'")
        } else if value.chars().all(|c| {
            c.is_ascii_graphic() && !c.is_whitespace() && c != '"' && c != '\'' && c != '\\'
        }) {
            // No quoting needed for simple values
            value.to_string()
        } else {
            // Double quotes for other cases
            format!("\"{}\"", value.replace('"', "\\\""))
        }
    }
}

impl Shell for Ion {
    fn create_quoter<'a>(&self, data: Quotable<'a>) -> Quoter<'a> {
        Quoter::new(
            data,
            QuoterOptions {
                on_quote: Arc::new(|data| Ion::do_quote(quotable_into_string(data))),
                ..Default::default()
            },
        )
    }

    // https://doc.redox-os.org/ion-manual/variables/05-exporting.html
    fn format(&self, statement: Statement<'_>) -> String {
        match statement {
            Statement::ModifyPath {
                paths,
                key,
                orig_key,
            } => {
                let key = key.unwrap_or("PATH");
                let value = paths.join(":");

                match orig_key {
                    Some(orig_key) => format!(r#"export {key} = "{value}:${{env::{orig_key}}}""#,),
                    None => format!(r#"export {key} = "{value}""#,),
                }
            }
            Statement::SetEnv { key, value } => {
                format!("export {}={}", self.quote(key), self.quote(value))
            }
            Statement::UnsetEnv { key } => {
                format!("drop {}", self.quote(key))
            }
        }
    }

    fn get_config_path(&self, home_dir: &Path) -> PathBuf {
        get_config_dir(home_dir).join("ion").join("initrc")
    }

    fn get_env_path(&self, home_dir: &Path) -> PathBuf {
        self.get_config_path(home_dir)
    }

    // https://doc.redox-os.org/ion-manual/general.html#xdg-app-dirs-support
    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf> {
        ProfileSet::default()
            .insert(get_config_dir(home_dir).join("ion").join("initrc"), 1)
            .insert(home_dir.join(".config").join("ion").join("initrc"), 2)
            .into_list()
    }
}

impl fmt::Display for Ion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ion")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_env_var() {
        assert_eq!(
            Ion.format_env_set("PROTO_HOME", "$HOME/.proto"),
            r#"export PROTO_HOME="$HOME/.proto""#
        );
    }

    #[test]
    fn formats_path_prepend() {
        assert_eq!(
            Ion.format_path_prepend(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"export PATH = "$PROTO_HOME/shims:$PROTO_HOME/bin:${env::PATH}""#
        );
    }

    #[test]
    fn formats_path_set() {
        assert_eq!(
            Ion.format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"export PATH = "$PROTO_HOME/shims:$PROTO_HOME/bin""#
        );
    }

    #[test]
    fn test_profile_paths() {
        #[allow(deprecated)]
        let home_dir = std::env::home_dir().unwrap();

        assert_eq!(
            Ion::new().get_profile_paths(&home_dir),
            vec![home_dir.join(".config").join("ion").join("initrc")]
        );
    }

    #[test]
    fn test_ion_quoting() {
        assert_eq!(Ion.quote("simplevalue"), "simplevalue");
        assert_eq!(Ion.quote("value with spaces"), r#""value with spaces""#);
        assert_eq!(
            Ion.quote(r#"value "with" quotes"#),
            r#""value \"with\" quotes""#
        );
        assert_eq!(Ion.quote("$variable"), "\"$variable\"");
        assert_eq!(Ion.quote("{brace_expansion}"), "'{brace_expansion}'");
        assert_eq!(
            Ion.quote("value with 'single quotes'"),
            r#""value with 'single quotes'""#
        );
    }
}
