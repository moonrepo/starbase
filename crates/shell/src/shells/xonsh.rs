use super::Shell;
use crate::helpers::{ProfileSet, get_config_dir, get_env_var_regex, normalize_newlines};
use crate::hooks::*;
use crate::quoter::*;
use shell_quote::Quotable;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Xonsh;

impl Xonsh {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self
    }

    /// Format a string expression while preserving `$VAR` environment references.
    fn format_string(&self, value: &str) -> String {
        let env_regex = get_env_var_regex();
        let mut output = String::with_capacity(value.len() + 3);
        let mut last = 0;

        output.push_str("f\"");

        for env_match in env_regex.find_iter(value) {
            Self::push_string_literal(&mut output, &value[last..env_match.start()]);
            output.push('{');
            output.push_str(env_match.as_str());
            output.push('}');
            last = env_match.end();
        }

        Self::push_string_literal(&mut output, &value[last..]);
        output.push('"');
        output
    }

    fn push_string_literal(output: &mut String, value: &str) {
        for ch in value.chars() {
            match ch {
                '\0' => output.push_str("\\0"),
                '\t' => output.push_str("\\t"),
                '\n' => output.push_str("\\n"),
                '\r' => output.push_str("\\r"),
                '"' => output.push_str("\\\""),
                '\\' => output.push_str("\\\\"),
                '{' => output.push_str("{{"),
                '}' => output.push_str("}}"),
                _ => output.push(ch),
            }
        }
    }
}

// https://xon.sh/bash_to_xsh.html
// https://xon.sh/xonshrc.html
impl Shell for Xonsh {
    fn create_quoter<'a>(&self, data: Quotable<'a>) -> Quoter<'a> {
        let mut options = QuoterOptions::default();
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
                let value = paths
                    .iter()
                    .map(|path| self.format_string(path))
                    .collect::<Vec<_>>()
                    .join(", ");

                match orig_key {
                    Some(orig_key) => format!(r#"${key} = [{value}] + ${orig_key}"#),
                    None => format!(r#"${key} = [{value}]"#),
                }
            }
            Statement::SetAlias { name, value } => {
                // The right-hand side is Python, not a shell word, so the value
                // must be a string literal, like `SetEnv` below
                format!("aliases[\"{name}\"] = {}", self.format_string(value))
            }
            Statement::SetEnv { key, value } => {
                format!("${key} = {}", self.format_string(value))
            }
            // `del` raises a `KeyError` when the alias/variable doesn't exist,
            // which would abort the entire block of statements
            Statement::UnsetAlias { name } => {
                format!("aliases.pop(\"{name}\", None)")
            }
            Statement::UnsetEnv { key } => {
                format!("${{...}}.pop(\"{key}\", None)")
            }
        }
    }

    // https://xon.sh/events.html
    fn format_hook(&self, hook: Hook) -> Result<String, crate::ShellError> {
        Ok(normalize_newlines(match hook {
            Hook::OnContextChange {
                activate_command,
                activate_function,
                deactivate_command,
                deactivate_function,
            } => {
                format!(
                    r#"
def {activate_function}(olddir=None, newdir=None, **kwargs):
    output = $({activate_command})
    if output:
        execx(output)

def {deactivate_function}():
    output = $({deactivate_command})
    if output:
        execx(output)
    for event in (events.on_chdir, events.on_pre_prompt):
        for handler in list(event):
            if getattr(handler, '__name__', '') == '{activate_function}':
                event.discard(handler)
    __xonsh__.ctx.pop('{activate_function}', None)
    __xonsh__.ctx.pop('{deactivate_function}', None)

# Re-sourcing creates new function objects, so deduplicate by name
if not any(getattr(handler, '__name__', '') == '{activate_function}' for handler in events.on_chdir):
    events.on_chdir({activate_function})

if not any(getattr(handler, '__name__', '') == '{activate_function}' for handler in events.on_pre_prompt):
    events.on_pre_prompt({activate_function})

# execx() does not evaluate into the shell namespace, so export both functions
__xonsh__.ctx['{activate_function}'] = {activate_function}
__xonsh__.ctx['{deactivate_function}'] = {deactivate_function}
"#
                )
            }
        }))
    }

    fn get_config_path(&self, home_dir: &Path) -> PathBuf {
        get_config_dir(home_dir).join("xonsh").join("rc.xsh")
    }

    fn get_env_path(&self, home_dir: &Path) -> PathBuf {
        self.get_config_path(home_dir)
    }

    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf> {
        ProfileSet::default()
            .insert(get_config_dir(home_dir).join("xonsh").join("rc.xsh"), 1)
            .insert(home_dir.join(".config").join("xonsh").join("rc.xsh"), 2)
            .insert(home_dir.join(".xonshrc"), 3)
            .into_list()
    }
}

impl fmt::Display for Xonsh {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "xonsh")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use starbase_sandbox::assert_snapshot;

    #[test]
    fn formats_env_var() {
        assert_eq!(
            Xonsh.format_env_set("PROTO_HOME", "$HOME/.proto"),
            r#"$PROTO_HOME = f"{$HOME}/.proto""#
        );
        assert_eq!(Xonsh.format_env_set("BOOL", "true"), r#"$BOOL = f"true""#);
        assert_eq!(Xonsh.format_env_set("FOO", "don't"), r#"$FOO = f"don't""#);
    }

    #[test]
    fn formats_context_change_hook() {
        let hook = Hook::OnContextChange {
            activate_command: "starbase hook xonsh".into(),
            activate_function: "_starbase_hook".into(),
            deactivate_command: "starbase deactivate xonsh".into(),
            deactivate_function: "_starbase_deactivate".into(),
        };

        assert_snapshot!(Xonsh.format_hook(hook).unwrap());
    }

    #[test]
    fn formats_path_prepend() {
        assert_eq!(
            Xonsh.format_path_prepend(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"$PATH = [f"{$PROTO_HOME}/shims", f"{$PROTO_HOME}/bin"] + $PATH"#
        );
    }

    #[test]
    fn formats_path_set() {
        assert_eq!(
            Xonsh.format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"$PATH = [f"{$PROTO_HOME}/shims", f"{$PROTO_HOME}/bin"]"#
        );
    }

    #[test]
    fn escapes_xonsh_string_literals() {
        assert_eq!(
            Xonsh.format_env_set("VALUE", "a {value} with \\\"quotes\\\""),
            r#"$VALUE = f"a {{value}} with \\\"quotes\\\"""#
        );
    }

    #[test]
    fn test_profile_paths() {
        #[allow(deprecated)]
        let home_dir = std::env::home_dir().unwrap();

        assert_eq!(
            Xonsh::new().get_profile_paths(&home_dir),
            vec![
                home_dir.join(".config").join("xonsh").join("rc.xsh"),
                home_dir.join(".xonshrc"),
            ]
        );
    }

    #[test]
    fn formats_alias_set() {
        assert_eq!(
            Xonsh.format_alias_set("ll", "ls -la"),
            r#"aliases["ll"] = f"ls -la""#
        );
        assert_eq!(
            Xonsh.format_alias_set("proto", "$PROTO_HOME/bin/proto"),
            r#"aliases["proto"] = f"{$PROTO_HOME}/bin/proto""#
        );
        assert_eq!(
            Xonsh.format_alias_set("brace", "echo {value}"),
            r#"aliases["brace"] = f"echo {{value}}""#
        );
    }

    #[test]
    fn formats_alias_unset() {
        assert_eq!(Xonsh.format_alias_unset("ll"), r#"aliases.pop("ll", None)"#);
    }

    #[test]
    fn formats_env_unset() {
        assert_eq!(
            Xonsh.format_env_unset("PROTO_VERSION"),
            r#"${...}.pop("PROTO_VERSION", None)"#
        );
    }

    #[test]
    fn test_xonsh_quoting() {
        let xonsh = Xonsh::new();
        assert_eq!(xonsh.quote(""), "''");
        assert_eq!(xonsh.quote("simple"), "simple");
        assert_eq!(xonsh.quote("don't"), "\"don't\"");
        assert_eq!(xonsh.quote("say \"hello\""), "\"say \\\"hello\\\"\"");
        assert_eq!(xonsh.quote("price $5"), "\"price $5\"");
        assert_eq!(
            xonsh.quote("complex 'value' with \"quotes\" and \\backslashes\\"),
            "\"complex 'value' with \\\"quotes\\\" and \\\\backslashes\\\\\""
        );
    }
}
