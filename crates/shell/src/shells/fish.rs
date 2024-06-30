use super::Shell;
use crate::helpers::get_config_dir;
use crate::hooks::Hook;
use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Fish;

impl Fish {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self
    }
}

// https://fishshell.com/docs/current/language.html#configuration
impl Shell for Fish {
    fn format_env_set(&self, key: &str, value: &str) -> String {
        format!("set -gx {} {};", self.quote(key), self.quote(value))
    }

    fn format_env_unset(&self, key: &str) -> String {
        format!("set -ge {};", self.quote(key))
    }

    fn format_path_set(&self, paths: &[String]) -> String {
        format!(r#"set -gx PATH "{}" $PATH;"#, paths.join(":"))
    }

    fn format_hook(&self, hook: Hook) -> Result<String, crate::ShellError> {
        Ok(hook.render_template(
            self,
            r#"
function __{prefix}_hook --on-variable PWD;
{export_env}
{export_path}
end;"#,
            "    ",
        ))
    }

    fn get_config_path(&self, home_dir: &Path) -> PathBuf {
        get_config_dir(home_dir).join("fish").join("config.fish")
    }

    fn get_env_path(&self, home_dir: &Path) -> PathBuf {
        self.get_config_path(home_dir)
    }

    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf> {
        HashSet::<PathBuf>::from_iter([
            get_config_dir(home_dir).join("fish").join("config.fish"),
            home_dir.join(".config").join("fish").join("config.fish"),
        ])
        .into_iter()
        .collect()
    }

    /// Quotes a string according to Fish shell quoting rules.
    ///
    /// This method handles quoting and escaping according to Fish shell rules.
    ///
    /// # Arguments
    ///
    /// * `value` - The string to be quoted.
    ///
    /// # Returns
    ///
    /// A quoted string suitable for use in Fish shell scripts.
    fn quote(&self, value: &str) -> String {
        if value.is_empty() {
            return "''".to_string();
        }

        // Check for complex values requiring special quoting
        if value.contains('\n')
            || value.contains('\t')
            || value.contains('\x07')
            || value.contains('\x08')
            || value.contains('\x1b')
            || value.contains('\x0c')
            || value.contains('\x0a')
            || value.contains('\x0d')
            || value.contains('\x0b')
            || value.contains('*')
            || value.contains('?')
            || value.contains('~')
            || value.contains('#')
            || value.contains('(')
            || value.contains(')')
            || value.contains('{')
            || value.contains('}')
            || value.contains('[')
            || value.contains(']')
            || value.contains('<')
            || value.contains('>')
            || value.contains('^')
            || value.contains('&')
            || value.contains('|')
            || value.contains(';')
            || value.contains('"')
            || value.contains('$')
        {
            format!(
                r#""{}""#,
                value
                    .replace("\\", "\\\\")
                    .replace("\n", "\\n")
                    .replace("\t", "\\t")
                    .replace("\x07", "\\a")
                    .replace("\x08", "\\b")
                    .replace("\x1b", "\\e")
                    .replace("\x0c", "\\f")
                    .replace("\x0a", "\\n")
                    .replace("\x0d", "\\r")
                    .replace("\x0b", "\\v")
                    .replace("\"", "\\\"")
                    .replace("$", "\\$")
                    .replace("*", "\\*")
                    .replace("?", "\\?")
                    .replace("~", "\\~")
                    .replace("#", "\\#")
                    .replace("(", "\\(")
                    .replace(")", "\\)")
                    .replace("{", "\\{")
                    .replace("}", "\\}")
                    .replace("[", "\\[")
                    .replace("]", "\\]")
                    .replace("<", "\\<")
                    .replace(">", "\\>")
                    .replace("^", "\\^")
                    .replace("&", "\\&")
                    .replace("|", "\\|")
                    .replace(";", "\\;")
            )
        } else if value.contains(' ') {
            format!("'{}'", value.replace("'", "''"))
        } else {
            value.to_string()
        }
    }
}
impl fmt::Display for Fish {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "fish")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use starbase_sandbox::assert_snapshot;

    #[test]
    fn formats_env_var() {
        assert_eq!(
            Fish.format_env_set("PROTO_HOME", "$HOME/.proto"),
            r#"set -gx PROTO_HOME "$HOME/.proto";"#
        );
    }

    #[test]
    fn formats_path() {
        assert_eq!(
            Fish.format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"set -gx PATH "$PROTO_HOME/shims:$PROTO_HOME/bin" $PATH;"#
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

        assert_snapshot!(Fish.format_hook(hook).unwrap());
    }

    #[test]
    fn test_fish_quoting() {
        assert_eq!(Fish.quote("\n"), r#""\n""#);
        assert_eq!(Fish.quote("\t"), r#""\t""#);
        assert_eq!(Fish.quote("\x07"), r#""\a""#);
        assert_eq!(Fish.quote("\x08"), r#""\b""#);
        assert_eq!(Fish.quote("\x1b"), r#""\e""#);
        assert_eq!(Fish.quote("\x0c"), r#""\f""#);
        assert_eq!(Fish.quote("\r"), r#""\r""#);
        assert_eq!(Fish.quote("\x0a"), r#""\n""#);
        assert_eq!(Fish.quote("\x0b"), r#""\v""#);
        assert_eq!(Fish.quote("*"), r#""\*""#);
        assert_eq!(Fish.quote("?"), r#""\?""#);
        assert_eq!(Fish.quote("~"), r#""\~""#);
        assert_eq!(Fish.quote("#"), r#""\#""#);
        assert_eq!(Fish.quote("("), r#""\(""#);
        assert_eq!(Fish.quote(")"), r#""\)""#);
        assert_eq!(Fish.quote("{"), r#""\{""#);
        assert_eq!(Fish.quote("}"), r#""\}""#);
        assert_eq!(Fish.quote("["), r#""\[""#);
        assert_eq!(Fish.quote("]"), r#""\]""#);
        assert_eq!(Fish.quote("<"), r#""\<""#);
        assert_eq!(Fish.quote(">"), r#""\>""#);
        assert_eq!(Fish.quote("^"), r#""\^""#);
        assert_eq!(Fish.quote("&"), r#""\&""#);
        assert_eq!(Fish.quote("|"), r#""\|""#);
        assert_eq!(Fish.quote(";"), r#""\;""#);
        assert_eq!(Fish.quote("\""), r#""\"""#);
        assert_eq!(Fish.quote("$"), r#""\$""#);
        assert_eq!(Fish.quote("$variable"), r#""\$variable""#);
        assert_eq!(Fish.quote("value with spaces"), "'value with spaces'");
    }
}
