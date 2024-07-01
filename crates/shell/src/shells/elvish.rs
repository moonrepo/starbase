use super::Shell;
use crate::helpers::{get_config_dir, get_env_var_regex};
use crate::hooks::Hook;
use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Elvish;

impl Elvish {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self
    }
}

fn format(value: impl AsRef<str>) -> String {
    get_env_var_regex()
        .replace_all(value.as_ref(), "$$E:$name")
        .replace("$E:HOME", "{~}")
}

// https://elv.sh/ref/command.html#using-elvish-interactivelyn
impl Shell for Elvish {
    fn format_env_set(&self, key: &str, value: &str) -> String {
        format!(
            "set-env {} {};",
            self.quote(key),
            self.quote(&format(value)).as_str()
        )
    }

    fn format_env_unset(&self, key: &str) -> String {
        format!("unset-env {};", self.quote(key))
    }

    fn format_path_set(&self, paths: &[String]) -> String {
        format!("set paths = [{} $@paths];", format(paths.join(" ")))
    }

    fn format_hook(&self, hook: Hook) -> Result<String, crate::ShellError> {
        Ok(hook.render_template(
            self,
            r#"
# {prefix} hook
set @edit:before-readline = $@edit:before-readline {
{export_env}
{export_path}
}"#,
            "  ",
        ))
    }

    fn get_config_path(&self, home_dir: &Path) -> PathBuf {
        get_config_dir(home_dir).join("elvish").join("rc.elv")
    }

    fn get_env_path(&self, home_dir: &Path) -> PathBuf {
        self.get_config_path(home_dir)
    }

    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf> {
        #[allow(unused_mut)]
        let mut profiles = HashSet::<PathBuf>::from_iter([
            get_config_dir(home_dir).join("elvish").join("rc.elv"),
            home_dir.join(".config").join("elvish").join("rc.elv"),
            home_dir.join(".elvish").join("rc.elv"), // Legacy
        ]);

        #[cfg(windows)]
        {
            profiles.insert(
                home_dir
                    .join("AppData")
                    .join("Roaming")
                    .join("elvish")
                    .join("rc.elv"),
            );
        }

        profiles.into_iter().collect()
    }

    /// Quotes a string according to Elvish shell quoting rules.
    /// @see <https://elv.sh/ref/language.html#single-quoted-string>
    #[allow(clippy::no_effect_replace)]
    fn quote(&self, value: &str) -> String {
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

    #[test]
    fn formats_path() {
        assert_eq!(
            Elvish.format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            "set paths = [$E:PROTO_HOME/shims $E:PROTO_HOME/bin $@paths];"
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

        assert_snapshot!(Elvish.format_hook(hook).unwrap());
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
