use super::Shell;
use crate::helpers::{
    PATH_DELIMITER, ProfileSet, get_config_dir, get_env_var_regex, normalize_newlines,
    quotable_into_string,
};
use crate::hooks::*;
use crate::quoter::*;
use shell_quote::Quotable;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub struct Elvish;

impl Elvish {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self
    }

    /// Quotes a string according to Elvish shell quoting rules.
    /// @see <https://elv.sh/ref/language.html#single-quoted-string>
    #[allow(clippy::no_effect_replace)]
    fn do_quote(value: String) -> String {
        // Check if the value is a bareword (only specific characters allowed)
        let is_bareword = value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-._:@/%~=+".contains(c));

        if is_bareword {
            // Barewords: no quotes needed
            value.to_string()
        } else if value.contains("{~}") {
            // Special case for {~} within the value to escape quoting
            value.to_string()
        } else if value.chars().any(|c| {
            c.is_whitespace()
                || [
                    '$', '"', '`', '\\', '\n', '\t', '\x07', '\x08', '\x0C', '\r', '\x1B', '\x7F',
                ]
                .contains(&c)
        }) {
            // Double-quoted strings with escape sequences
            format!(
                r#""{}""#,
                value
                    .replace('\\', "\\\\")
                    .replace('\n', "\\n")
                    .replace('\t', "\\t")
                    .replace('\x07', "\\a")
                    .replace('\x08', "\\b")
                    .replace('\x0C', "\\f")
                    .replace('\r', "\\r")
                    .replace('\x1B', "\\e")
                    .replace('\"', "\\\"")
                    .replace('\x7F', "\\^?")
                    .replace('\0', "\x00")
            )
        } else {
            // Single-quoted strings for non-barewords containing special characters
            format!("'{}'", value.replace('\'', "''").replace('\0', "\x00"))
        }
    }
}

fn format(value: impl AsRef<str>) -> String {
    get_env_var_regex()
        .replace_all(value.as_ref(), "$$E:$name")
        .replace("$E:HOME", "{~}")
}

// https://elv.sh/ref/command.html#using-elvish-interactivelyn
impl Shell for Elvish {
    fn create_quoter<'a>(&self, data: Quotable<'a>) -> Quoter<'a> {
        Quoter::new(
            data,
            QuoterOptions {
                unquoted_syntax: vec![Syntax::Symbol("{~}".into())],
                on_quote: Arc::new(|data| Elvish::do_quote(quotable_into_string(data))),
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
                let value = format(
                    paths
                        .iter()
                        .map(|p| self.quote(p))
                        .collect::<Vec<_>>()
                        .join(" "),
                );

                match orig_key {
                    Some(orig_key) => {
                        if orig_key == "PATH" {
                            format!("set paths = [{value} $@paths];")
                        } else {
                            format!(
                                r#"set-env {key} "{}{PATH_DELIMITER}"$E:{orig_key};"#,
                                paths.join(PATH_DELIMITER)
                            )
                        }
                    }
                    None => format!("set paths = [{value}];"),
                }
            }
            Statement::SetEnv { key, value } => {
                format!(
                    "set-env {} {};",
                    self.quote(key),
                    self.quote(&format(value)).as_str()
                )
            }
            Statement::UnsetEnv { key } => {
                format!("unset-env {};", self.quote(key))
            }
        }
    }

    fn format_hook(&self, hook: Hook) -> Result<String, crate::ShellError> {
        Ok(normalize_newlines(match hook {
            Hook::OnChangeDir { command, function } => {
                format!(
                    r#"
set-env __ORIG_PATH $E:PATH

fn {function} {{
  eval ({command});
}}

set @edit:before-readline = $@edit:before-readline {{
  {function}
}}
"#
                )
            }
        }))
    }

    fn get_config_path(&self, home_dir: &Path) -> PathBuf {
        get_config_dir(home_dir).join("elvish").join("rc.elv")
    }

    fn get_env_path(&self, home_dir: &Path) -> PathBuf {
        self.get_config_path(home_dir)
    }

    // https://elv.sh/ref/command.html#rc-file
    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf> {
        let mut profiles = ProfileSet::default()
            .insert(get_config_dir(home_dir).join("elvish").join("rc.elv"), 1)
            .insert(home_dir.join(".config").join("elvish").join("rc.elv"), 2);

        #[cfg(windows)]
        {
            profiles = profiles.insert(
                home_dir
                    .join("AppData")
                    .join("Roaming")
                    .join("elvish")
                    .join("rc.elv"),
                3,
            );
        }

        profiles = profiles.insert(home_dir.join(".elvish").join("rc.elv"), 4); // Legacy
        profiles.into_list()
    }
}

impl fmt::Display for Elvish {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "elvish")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use starbase_sandbox::assert_snapshot;

    #[test]
    fn formats_env_var() {
        assert_eq!(
            Elvish.format_env_set("PROTO_HOME", "$HOME/.proto"),
            "set-env PROTO_HOME {~}/.proto;"
        );
        assert_eq!(Elvish.format_env_set("FOO", "bar"), "set-env FOO bar;");
    }

    #[cfg(unix)]
    #[test]
    fn formats_path_prepend() {
        assert_eq!(
            Elvish.format_path_prepend(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"set paths = ["$E:PROTO_HOME/shims" "$E:PROTO_HOME/bin" $@paths];"#
        );
    }

    #[cfg(windows)]
    #[test]
    fn formats_path_prepend() {
        assert_eq!(
            Elvish.format_path_prepend(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"set paths = ["$E:PROTO_HOME/shims" "$E:PROTO_HOME/bin" $@paths];"#
        );
    }

    #[test]
    fn formats_path_set() {
        assert_eq!(
            Elvish.format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"set paths = ["$E:PROTO_HOME/shims" "$E:PROTO_HOME/bin"];"#
        );
    }

    #[test]
    fn formats_cd_hook() {
        let hook = Hook::OnChangeDir {
            command: "starbase hook elvish".into(),
            function: "_starbase_hook".into(),
        };

        assert_snapshot!(Elvish.format_hook(hook).unwrap());
    }

    #[test]
    fn test_profile_paths() {
        #[allow(deprecated)]
        let home_dir = std::env::home_dir().unwrap();

        if cfg!(windows) {
            assert_eq!(
                Elvish::new().get_profile_paths(&home_dir),
                vec![
                    home_dir.join(".config").join("elvish").join("rc.elv"),
                    home_dir
                        .join("AppData")
                        .join("Roaming")
                        .join("elvish")
                        .join("rc.elv"),
                    home_dir.join(".elvish").join("rc.elv"),
                ]
            );
        } else {
            assert_eq!(
                Elvish::new().get_profile_paths(&home_dir),
                vec![
                    home_dir.join(".config").join("elvish").join("rc.elv"),
                    home_dir.join(".elvish").join("rc.elv"),
                ]
            );
        }
    }

    #[test]
    fn test_elvish_quoting() {
        // Barewords
        assert_eq!(Elvish.quote("simple"), "simple");
        assert_eq!(Elvish.quote("a123"), "a123");
        assert_eq!(Elvish.quote("foo_bar"), "foo_bar");
        assert_eq!(Elvish.quote("A"), "A");

        // Single quotes
        assert_eq!(Elvish.quote("it's"), "'it''s'");
        assert_eq!(Elvish.quote("value'with'quotes"), "'value''with''quotes'");

        // Double quotes
        assert_eq!(Elvish.quote("value with spaces"), r#""value with spaces""#);
        assert_eq!(
            Elvish.quote("value\"with\"quotes"),
            r#""value\"with\"quotes""#
        );
        assert_eq!(
            Elvish.quote("value\nwith\nnewlines"),
            r#""value\nwith\nnewlines""#
        );
        assert_eq!(Elvish.quote("value\twith\ttabs"), r#""value\twith\ttabs""#);
        assert_eq!(
            Elvish.quote("value\\with\\backslashes"),
            r#""value\\with\\backslashes""#
        );

        // Escape sequences
        assert_eq!(Elvish.quote("\x41"), "A"); // A is a bareword
        assert_eq!(Elvish.quote("\u{0041}"), "A"); // A is a bareword
        assert_eq!(Elvish.quote("\x09"), r#""\t""#);
        assert_eq!(Elvish.quote("\x07"), r#""\a""#);
        assert_eq!(Elvish.quote("\x1B"), r#""\e""#);
        assert_eq!(Elvish.quote("\x7F"), r#""\^?""#);

        // Unsupported sequences
        assert_eq!(Elvish.quote("\0"), "'\x00'".to_string());
    }
}
