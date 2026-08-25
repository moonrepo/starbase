use super::Shell;
use crate::helpers::{normalize_newlines, render_template};
use crate::hooks::*;
use crate::quoter::*;
use shell_quote::{Quotable, Sh as ShQuoter};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub struct Sh;

impl Sh {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self
    }
}

impl Shell for Sh {
    fn create_quoter<'a>(&self, data: Quotable<'a>) -> Quoter<'a> {
        Quoter::new(
            data,
            QuoterOptions {
                replacements_expansion: posix_expansion_escape_chars(),
                on_quote: Some(Arc::new(|data| {
                    String::from_utf8_lossy(&ShQuoter::quote_vec(data)).into()
                })),
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
                let mut value = paths.join(":");

                if let Some(orig) = orig_key {
                    value.push_str(":$");
                    value.push_str(orig);
                }

                format!(r#"export {key}="{value}";"#)
            }
            Statement::SetAlias { name, value, .. } => {
                format!("alias {}={};", self.quote(name), self.quote(value))
            }
            Statement::SetEnv { key, value } => {
                format!("export {}={};", self.quote(key), self.quote(value))
            }
            Statement::UnsetAlias { name, .. } => {
                format!("unalias {};", self.quote(name))
            }
            Statement::UnsetEnv { key } => {
                format!("unset {};", self.quote(key))
            }
        }
    }

    // POSIX shells have no native change-dir or prompt hook, so shadow the
    // `cd` builtin with a function. Redefining the function is naturally
    // idempotent, and deactivation restores the builtin with `unset -f`.
    // Caveat: this clobbers any other tool's `cd` wrapper, as POSIX offers
    // no way to introspect or chain an existing function.
    fn format_hook(&self, hook: Hook) -> Result<String, crate::ShellError> {
        Ok(normalize_newlines(match hook {
            Hook::OnContextChange {
                activate_command,
                activate_function,
                deactivate_command,
                deactivate_function,
            } => render_template(
                include_str!("hooks/sh.sh"),
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
        home_dir.join(".profile")
    }

    fn get_env_path(&self, home_dir: &Path) -> PathBuf {
        home_dir.join(".profile")
    }

    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf> {
        vec![home_dir.join(".profile")]
    }
}

impl fmt::Display for Sh {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "sh")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use starbase_sandbox::assert_snapshot;

    #[test]
    fn formats_env_var() {
        assert_eq!(
            Sh.format_env_set("PROTO_HOME", "$HOME/.proto"),
            r#"export PROTO_HOME="$HOME/.proto";"#
        );
    }

    #[test]
    fn formats_context_change_hook() {
        let hook = Hook::OnContextChange {
            activate_command: "starbase hook sh".into(),
            activate_function: "_starbase_hook".into(),
            deactivate_command: "starbase deactivate sh".into(),
            deactivate_function: "_starbase_deactivate".into(),
        };

        assert_snapshot!(Sh.format_hook(hook).unwrap());
    }

    #[test]
    fn formats_path_prepend() {
        assert_eq!(
            Sh.format_path_prepend(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"export PATH="$PROTO_HOME/shims:$PROTO_HOME/bin:$PATH";"#
        );
    }

    #[test]
    fn formats_path_set() {
        assert_eq!(
            Sh.format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"export PATH="$PROTO_HOME/shims:$PROTO_HOME/bin";"#
        );
    }

    #[test]
    fn formats_alias_set() {
        assert_eq!(Sh.format_alias_set("ll", "ls -la"), "alias ll=ls' -la';");
    }

    #[test]
    fn formats_alias_unset() {
        assert_eq!(Sh.format_alias_unset("ll"), "unalias ll;");
    }

    #[test]
    fn test_sh_quoting() {
        let sh = Sh::new();
        assert_eq!(sh.quote(""), "''");
        assert_eq!(sh.quote("simple"), "simple");
        assert_eq!(sh.quote("say \"hello\""), "\"say \\\"hello\\\"\"");
        assert_eq!(sh.quote("price $5"), "\"price $5\"");
        assert_eq!(
            sh.quote("complex 'value' with \"quotes\" and \\backslashes\\"),
            "\"complex 'value' with \\\"quotes\\\" and \\\\backslashes\\\\\""
        );
    }
}
