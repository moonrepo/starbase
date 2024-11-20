use super::Shell;
use crate::helpers::get_config_dir;
use crate::helpers::ProfileSet;
use crate::hooks::*;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Ion;

impl Ion {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self
    }
}

impl Shell for Ion {
    // https://doc.redox-os.org/ion-manual/variables/05-exporting.html
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
                    r#"export {key} = "{}:${{env::{orig_key}}}""#,
                    paths.join(":"),
                )
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

    /// Quotes a string according to Ion shell quoting rules.
    /// @see <https://doc.redox-os.org/ion-manual/general.html>
    fn quote(&self, value: &str) -> String {
        if value.starts_with('$') {
            // Variables expanded in double quotes
            format!("\"{}\"", value)
        } else if value.contains('{') || value.contains('}') {
            // Single quotes to prevent brace expansion
            format!("'{}'", value)
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
    fn formats_path() {
        assert_eq!(
            Ion.format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"export PATH = "$PROTO_HOME/shims:$PROTO_HOME/bin:${env::PATH}""#
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
