use super::Shell;
use crate::helpers::{
    PATH_DELIMITER, ProfileSet, get_config_dir, get_env_var_regex, normalize_newlines,
    render_template,
};
use crate::hooks::*;
use crate::quoter::*;
use shell_quote::Quotable;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Elvish;

impl Elvish {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self
    }

    // $FOO -> ${env::FOO}
    fn replace_env(&self, value: impl AsRef<str>) -> String {
        get_env_var_regex()
            .replace_all(value.as_ref(), "$$E:$name")
            .replace("$E:HOME", "{~}")
    }
}

// https://elv.sh/ref/command.html#using-elvish-interactivelyn
impl Shell for Elvish {
    fn create_quoter<'a>(&self, data: Quotable<'a>) -> Quoter<'a> {
        let mut options = QuoterOptions {
            quoted_syntax: vec![],
            // https://elv.sh/learn/tour.html#brace-expansion
            unquoted_syntax: vec![
                // brace
                Syntax::Pair("{".into(), "}".into()),
                // tilde
                Syntax::Symbol("{~}".into()),
                // file, glob
                Syntax::Symbol("**".into()),
                Syntax::Symbol("*".into()),
                Syntax::Symbol("?".into()),
            ],
            ..Default::default()
        };
        options.quote_pairs.push(("\"".into(), "\"".into(), false));

        Quoter::new(data, options)
    }

    fn format(&self, statement: Statement<'_>) -> String {
        match statement {
            Statement::ModifyPath {
                paths,
                key,
                orig_key,
            } => {
                let key = key.unwrap_or("PATH");
                let value = self.replace_env(
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
            // `@_` forwards the arguments, which a bare `fn` body would
            // reject. A hook evaluates its statements in a namespace that is
            // dropped when `eval` returns, so there the alias is added to the
            // interactive namespace instead, the way the hook adds its own
            // functions
            Statement::SetAlias { name, value, hook } => {
                if hook {
                    format!(
                        "try {{ edit:add-vars [&'{name}~'={{|@_| {value} $@_ }}] }} catch _ {{ }}"
                    )
                } else {
                    format!("fn {name} {{|@_| {value} $@_ }}")
                }
            }
            Statement::SetEnv { key, value } => {
                format!(
                    "set-env {} {};",
                    self.quote(key),
                    self.quote(self.replace_env(value).as_str())
                )
            }
            // An alias lives in the `{name}~` slot, not `{name}`. A hook
            // cannot use `del`, since a missing variable is a compilation
            // error, which would take down every statement sharing the `eval`
            // rather than just this one. `edit:del-vars` tolerates a name that
            // was never added
            Statement::UnsetAlias { name, hook } => {
                if hook {
                    format!("try {{ edit:del-vars ['{name}~'] }} catch _ {{ }}")
                } else {
                    format!("del {name}~")
                }
            }
            Statement::UnsetEnv { key } => {
                format!("unset-env {};", self.quote(key))
            }
        }
    }

    fn format_hook(&self, hook: Hook) -> Result<String, crate::ShellError> {
        Ok(normalize_newlines(match hook {
            Hook::OnContextChange {
                activate_command,
                activate_function,
                deactivate_command,
                deactivate_function,
            } => render_template(
                include_str!("hooks/elvish.elv"),
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
        get_config_dir(home_dir).join("elvish").join("rc.elv")
    }

    fn get_env_path(&self, home_dir: &Path) -> PathBuf {
        self.get_config_path(home_dir)
    }

    fn get_env_regex(&self) -> regex::Regex {
        regex::Regex::new(r"\$E:(?<name>[A-Za-z0-9_]+)").unwrap()
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
        assert_eq!(
            Elvish.format_env_set("FOO", "don't"),
            "set-env FOO \"don't\";"
        );
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
    fn formats_context_change_hook() {
        let hook = Hook::OnContextChange {
            activate_command: "starbase hook elvish".into(),
            activate_function: "_starbase_hook".into(),
            deactivate_command: "starbase deactivate elvish".into(),
            deactivate_function: "_starbase_deactivate".into(),
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
    fn formats_alias_set() {
        assert_eq!(
            Elvish.format_alias_set("ll", "ls -la"),
            "fn ll {|@_| ls -la $@_ }"
        );
    }

    #[test]
    fn formats_alias_unset() {
        // The alias is a function, which lives in the `~` slot
        assert_eq!(Elvish.format_alias_unset("ll"), "del ll~");
    }

    #[test]
    fn formats_hook_alias_set() {
        assert_eq!(
            Elvish.format(Statement::SetAlias {
                name: "ll",
                value: "ls -la",
                hook: true
            }),
            "try { edit:add-vars [&'ll~'={|@_| ls -la $@_ }] } catch _ { }"
        );
        assert_eq!(
            Elvish.format(Statement::SetAlias {
                name: "..",
                value: "cd ..",
                hook: true
            }),
            "try { edit:add-vars [&'..~'={|@_| cd .. $@_ }] } catch _ { }"
        );
    }

    #[test]
    fn formats_hook_alias_unset() {
        assert_eq!(
            Elvish.format(Statement::UnsetAlias {
                name: "ll",
                hook: true
            }),
            "try { edit:del-vars ['ll~'] } catch _ { }"
        );
    }

    #[test]
    fn test_elvish_quoting() {
        // Barewords
        assert_eq!(Elvish.quote("simple"), "simple");
        assert_eq!(Elvish.quote("a123"), "a123");
        assert_eq!(Elvish.quote("foo_bar"), "foo_bar");
        assert_eq!(Elvish.quote("A"), "A");

        // Single quotes
        assert_eq!(Elvish.quote("it's"), "\"it's\"");
        assert_eq!(Elvish.quote("value'with'quotes"), "\"value'with'quotes\"");

        // Double quotes
        assert_eq!(Elvish.quote("value with spaces"), r#"'value with spaces'"#);
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

        // Unsupported sequences
        assert_eq!(Elvish.quote("\0"), "\"\x00\"".to_string());
    }
}
