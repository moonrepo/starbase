use super::{Shell, ShellExt};
use crate::helpers::{ProfileSet, get_config_dir, get_env_var_regex};
use crate::hooks::*;
use crate::quoter::*;
use shell_quote::Quotable;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Ion;

impl Ion {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self
    }

    // $FOO -> ${env::FOO}
    fn replace_env(&self, value: impl AsRef<str>) -> String {
        get_env_var_regex()
            .replace_all(value.as_ref(), "$${env::$name}")
            .to_string()
    }
}

impl Shell for Ion {
    fn create_quoter<'a>(&self, data: Quotable<'a>) -> Quoter<'a> {
        Quoter::new(
            data,
            QuoterOptions {
                // https://github.com/redox-os/ion/blob/master/src/lib/expansion/methods/strings.rs
                // https://doc.redox-os.org/ion-manual/expansions/00-expansions.html
                quoted_syntax: vec![
                    Syntax::Symbol("$".into()),
                    Syntax::Pair("${".into(), "}".into()),
                    Syntax::Pair("$(".into(), ")".into()),
                    Syntax::Symbol("@".into()),
                    Syntax::Pair("@{".into(), "}".into()),
                    Syntax::Pair("@(".into(), ")".into()),
                ],
                unquoted_syntax: vec![
                    // brace
                    Syntax::Pair("{".into(), "}".into()),
                ],
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
                let value = self.replace_env(paths.join(":"));

                match orig_key {
                    Some(orig_key) => format!(r#"export {key} = "{value}:${{env::{orig_key}}}""#,),
                    None => format!(r#"export {key} = "{value}""#,),
                }
            }
            Statement::SetEnv { key, value } => {
                format!(
                    "export {}={}",
                    self.quote(key),
                    self.quote(self.replace_env(value).as_str())
                )
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

    fn get_env_regex(&self) -> regex::Regex {
        regex::Regex::new(r"\$\{env::(?<name>[A-Za-z0-9_]+)\}").unwrap()
    }

    // https://doc.redox-os.org/ion-manual/general.html#xdg-app-dirs-support
    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf> {
        ProfileSet::default()
            .insert(get_config_dir(home_dir).join("ion").join("initrc"), 1)
            .insert(home_dir.join(".config").join("ion").join("initrc"), 2)
            .into_list()
    }
}

impl ShellExt for Ion {}

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
            r#"export PROTO_HOME="${env::HOME}/.proto""#
        );
    }

    #[test]
    fn formats_path_prepend() {
        assert_eq!(
            Ion.format_path_prepend(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"export PATH = "${env::PROTO_HOME}/shims:${env::PROTO_HOME}/bin:${env::PATH}""#
        );
    }

    #[test]
    fn formats_path_set() {
        assert_eq!(
            Ion.format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"export PATH = "${env::PROTO_HOME}/shims:${env::PROTO_HOME}/bin""#
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
        assert_eq!(Ion.quote("value with spaces"), r#"'value with spaces'"#);
        assert_eq!(
            Ion.quote(r#"value "with" quotes"#),
            r#""value \"with\" quotes""#
        );
        assert_eq!(Ion.quote("$variable"), "\"$variable\"");
        assert_eq!(Ion.quote("{brace_expansion}"), "{brace_expansion}");
        assert_eq!(
            Ion.quote("value with 'single quotes'"),
            r#"'value with 'single quotes''"#
        );
    }
}
