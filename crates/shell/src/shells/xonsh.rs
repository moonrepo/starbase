use super::Shell;
use crate::helpers::{ProfileSet, get_config_dir, normalize_newlines};
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
}

// https://xon.sh/bash_to_xsh.html
// https://xon.sh/xonshrc.html
impl Shell for Xonsh {
    fn create_quoter<'a>(&self, data: Quotable<'a>) -> Quoter<'a> {
        Quoter::new(data, QuoterOptions::default())
    }

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
                    Some(orig_key) => format!(r#"${key} = "{value}:${orig_key}""#),
                    None => format!(r#"${key} = "{value}""#),
                }
            }
            Statement::SetAlias { name, value } => {
                format!("aliases[\"{name}\"] = {}", self.quote(value))
            }
            Statement::SetEnv { key, value } => {
                format!("${key} = {}", self.quote(value))
            }
            Statement::UnsetAlias { name } => {
                format!("del aliases[\"{name}\"]")
            }
            Statement::UnsetEnv { key } => {
                format!("del ${key}")
            }
        }
    }

    // https://xon.sh/events.html
    fn format_hook(&self, hook: Hook) -> Result<String, crate::ShellError> {
        Ok(normalize_newlines(match hook {
            Hook::OnChangeDir {
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
    global {activate_function}, {deactivate_function}
    output = $({deactivate_command})
    if output:
        execx(output)
    for handler in list(events.on_chdir):
        if getattr(handler, '__name__', '') == '{activate_function}':
            events.on_chdir.discard(handler)
    del {activate_function}, {deactivate_function}

# Re-sourcing creates new function objects, so deduplicate by name
if not any(getattr(handler, '__name__', '') == '{activate_function}' for handler in events.on_chdir):
    events.on_chdir({activate_function})
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
            r#"$PROTO_HOME = "$HOME/.proto""#
        );
    }

    #[test]
    fn formats_cd_hook() {
        let hook = Hook::OnChangeDir {
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
            r#"$PATH = "$PROTO_HOME/shims:$PROTO_HOME/bin:$PATH""#
        );
    }

    #[test]
    fn formats_path_set() {
        assert_eq!(
            Xonsh.format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"$PATH = "$PROTO_HOME/shims:$PROTO_HOME/bin""#
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
            "aliases[\"ll\"] = 'ls -la'"
        );
    }

    #[test]
    fn formats_alias_unset() {
        assert_eq!(Xonsh.format_alias_unset("ll"), "del aliases[\"ll\"]");
    }

    #[test]
    fn test_xonsh_quoting() {
        let xonsh = Xonsh::new();
        assert_eq!(xonsh.quote(""), "''");
        assert_eq!(xonsh.quote("simple"), "simple");
        assert_eq!(xonsh.quote("don't"), "'don't'");
        assert_eq!(xonsh.quote("say \"hello\""), "\"say \\\"hello\\\"\"");
        assert_eq!(xonsh.quote("price $5"), "\"price $5\"");
        assert_eq!(
            xonsh.quote("complex 'value' with \"quotes\" and \\backslashes\\"),
            "\"complex 'value' with \\\"quotes\\\" and \\\\backslashes\\\\\""
        );
    }
}
