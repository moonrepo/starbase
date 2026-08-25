use super::Shell;
use crate::helpers::{PATH_DELIMITER, get_env_var_regex, normalize_newlines, render_template};
use crate::hooks::*;
use crate::quoter::*;
use shell_quote::Quotable;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Murex;

impl Murex {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self
    }

    // $FOO -> $ENV.FOO
    fn replace_env(&self, value: impl AsRef<str>) -> String {
        get_env_var_regex()
            .replace_all(value.as_ref(), "$$ENV.$name")
            .to_string()
    }

    /// Quote a value for the right side of an assignment, which murex parses
    /// as an expression. A bareword is a syntax error there, so a value that
    /// needs no shell quoting still needs syntax quoting.
    fn quote_assignment(&self, value: &str) -> String {
        let quoter = self.create_quoter(value.into());

        // Interpolated and already quoted values keep the quoting they have
        if quoter.is_empty() || quoter.is_quoted() || quoter.requires_expansion() {
            self.quote(value)
        } else {
            self.create_quoter(value.into()).quote()
        }
    }
}

impl Shell for Murex {
    fn create_quoter<'a>(&self, data: Quotable<'a>) -> Quoter<'a> {
        let mut options = QuoterOptions::default();
        options.quote_pairs.push(("%(".into(), ")".into(), false));
        options.quote_pairs.push(("\"".into(), "\"".into(), false));

        Quoter::new(data, options)
    }

    fn format(&self, statement: Statement<'_>) -> String {
        match statement {
            Statement::ModifyPath {
                paths,
                key,
                orig_key,
                ..
            } => {
                let key = key.unwrap_or("PATH");
                let value = self.replace_env(paths.join(PATH_DELIMITER));

                match orig_key {
                    Some(orig_key) => {
                        format!(r#"$ENV.{key}="{value}{PATH_DELIMITER}$ENV.{orig_key}""#)
                    }
                    None => format!(r#"$ENV.{key}="{value}""#),
                }
            }
            Statement::SetAlias { name, value, .. } => {
                format!("alias {}={};", self.quote(name), self.quote(value))
            }
            Statement::SetEnv { key, value, .. } => {
                format!(
                    "$ENV.{}={}",
                    self.quote(key),
                    self.quote_assignment(self.replace_env(value).as_str())
                )
            }
            Statement::UnsetAlias { name, .. } => {
                format!("!alias {};", self.quote(name))
            }
            Statement::UnsetEnv { key, .. } => {
                format!("unset {};", self.quote(key))
            }
        }
    }

    // hook referenced from https://github.com/direnv/direnv/blob/ff451a860b31f176d252c410b43d7803ec0f8b23/internal/cmd/shell_murex.go#L12
    fn format_hook(&self, hook: Hook) -> Result<String, crate::ShellError> {
        Ok(normalize_newlines(match hook {
            Hook::OnContextChange {
                activate_command,
                activate_function,
                deactivate_command,
                deactivate_function,
            } => render_template(
                include_str!("hooks/murex.mx"),
                &[
                    ("activate_command", &activate_command),
                    ("activate_function", &activate_function),
                    ("deactivate_command", &deactivate_command),
                    ("deactivate_function", &deactivate_function),
                ],
            ),
        }))
    }

    fn get_config_path(&self, home_dir: &Path) -> PathBuf {
        home_dir.join(".murex_profile")
    }

    fn get_env_path(&self, home_dir: &Path) -> PathBuf {
        home_dir.join(".murex_preload")
    }

    fn get_env_regex(&self) -> regex::Regex {
        regex::Regex::new(r"\$ENV.(?<name>[A-Za-z0-9_]+)").unwrap()
    }

    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf> {
        vec![
            home_dir.join(".murex_profile"),
            home_dir.join(".murex_preload"),
        ]
    }
}

impl fmt::Display for Murex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "murex")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use starbase_sandbox::assert_snapshot;

    #[test]
    fn formats_env_var() {
        assert_eq!(
            Murex.format_env_set("PROTO_HOME", "$HOME/.proto"),
            r#"$ENV.PROTO_HOME="$ENV.HOME/.proto""#
        );
        assert_eq!(Murex.format_env_set("FOO", "don't"), "$ENV.FOO=%(don't)");
        // The expression parser rejects a bareword, so a plain value is quoted
        assert_eq!(Murex.format_env_set("BOOL", "true"), "$ENV.BOOL='true'");
        assert_eq!(Murex.format_env_set("EMPTY", ""), "$ENV.EMPTY=''");
    }

    #[cfg(unix)]
    #[test]
    fn formats_path_prepend() {
        assert_eq!(
            Murex.format_path_prepend(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"$ENV.PATH="$ENV.PROTO_HOME/shims:$ENV.PROTO_HOME/bin:$ENV.PATH""#
        );
    }

    #[cfg(unix)]
    #[test]
    fn formats_path_set() {
        assert_eq!(
            Murex.format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"$ENV.PATH="$ENV.PROTO_HOME/shims:$ENV.PROTO_HOME/bin""#
        );
    }

    #[cfg(windows)]
    #[test]
    fn formats_path_prepend() {
        assert_eq!(
            Murex.format_path_prepend(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"$ENV.PATH="$ENV.PROTO_HOME/shims;$ENV.PROTO_HOME/bin;$ENV.PATH""#
        );
    }

    #[cfg(windows)]
    #[test]
    fn formats_path_set() {
        assert_eq!(
            Murex.format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"$ENV.PATH="$ENV.PROTO_HOME/shims;$ENV.PROTO_HOME/bin""#
        );
    }

    #[test]
    fn formats_context_change_hook() {
        let hook = Hook::OnContextChange {
            activate_command: "starbase hook murex".into(),
            activate_function: "_starbase_hook".into(),
            deactivate_command: "starbase deactivate murex".into(),
            deactivate_function: "_starbase_deactivate".into(),
        };

        assert_snapshot!(Murex.format_hook(hook).unwrap());
    }

    #[test]
    fn test_profile_paths() {
        #[allow(deprecated)]
        let home_dir = std::env::home_dir().unwrap();

        assert_eq!(
            Murex::new().get_profile_paths(&home_dir),
            vec![
                home_dir.join(".murex_profile"),
                home_dir.join(".murex_preload"),
            ]
        );
    }

    #[test]
    fn formats_alias_set() {
        assert_eq!(Murex.format_alias_set("ll", "ls -la"), "alias ll='ls -la';");
    }

    #[test]
    fn formats_alias_unset() {
        assert_eq!(Murex.format_alias_unset("ll"), "!alias ll;");
    }

    #[test]
    fn test_murex_quoting() {
        assert_eq!(Murex.quote("value"), "value");
        assert_eq!(Murex.quote("value with spaces"), "'value with spaces'");
        assert_eq!(Murex.quote("don't"), "%(don't)");
        assert_eq!(Murex.quote("don't)"), "\"don't)\"");
        assert_eq!(Murex.quote("$(echo hello)"), "\"$(echo hello)\"");
        assert_eq!(Murex.quote(""), "''");
        assert_eq!(Murex.quote("abc123"), "abc123");
        assert_eq!(Murex.quote("%(Bob)"), "%(Bob)");
        assert_eq!(Murex.quote("%(hello world)"), "%(hello world)");
    }
}
