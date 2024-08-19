use super::Shell;
use crate::helpers::{get_config_dir, get_env_var_regex, normalize_newlines};
use crate::hooks::*;
use std::collections::HashSet;
use std::env::consts;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Nu;

impl Nu {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self
    }
}

fn join_path(value: impl AsRef<str>) -> String {
    let parts = value
        .as_ref()
        .split(['/', '\\'])
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();

    format!("path join {}", parts.join(" "))
}

impl Shell for Nu {
    // https://www.nushell.sh/book/configuration.html#environment
    fn format(&self, statement: Statement<'_>) -> String {
        let path_name = if consts::OS == "windows" {
            "Path"
        } else {
            "PATH"
        };

        match statement {
            Statement::PrependPath {
                paths,
                key,
                orig_key,
            } => {
                let env_regex = get_env_var_regex();
                let key = key.unwrap_or(path_name);
                let orig_key = orig_key.unwrap_or(key);
                let mut value = format!("$env.{key} = ($env.{orig_key} | split row (char esep)\n");

                // https://www.nushell.sh/book/configuration.html#path-configuration
                for path in paths.iter().rev() {
                    value.push_str("  | prepend ");

                    if let Some(cap) = env_regex.captures(path) {
                        let path_without_env = path.replace(cap.get(0).unwrap().as_str(), "");

                        value.push('(');
                        value.push_str(&format!("$env.{}", cap.name("name").unwrap().as_str()));
                        value.push_str(" | ");
                        value.push_str(&join_path(path_without_env));
                        value.push(')');
                    } else {
                        value.push_str(path);
                    }

                    value.push('\n');
                }

                value.push_str("  | uniq)");

                normalize_newlines(value)
            }
            Statement::SetEnv { key, value } => {
                if value.starts_with("$HOME/") {
                    let path = value.trim_start_matches("$HOME/");
                    format!("$env.{} = ($env.HOME | path join '{}')", key, path)
                } else {
                    format!("$env.{} = {}", key, self.quote(value))
                }
            }
            Statement::UnsetEnv { key } => {
                format!("hide-env {key}")
            }
        }
    }

    fn format_hook(&self, hook: Hook) -> Result<String, crate::ShellError> {
        let path_name = if consts::OS == "windows" {
            "Path"
        } else {
            "PATH"
        };

        Ok(normalize_newlines(match hook {
            Hook::OnChangeDir { command, prefix } => {
                format!(
                    r#"
# {prefix} hook
$env.__ORIG_PATH = $env.{path_name}

$env.config = ( $env.config | upsert hooks.env_change.PWD {{ |config|
  let list = ($config | get -i hooks.env_change.PWD) | default []

  $list | append {{ |before, after|
    let data = {command} | from json

    $data | get env | items {{ |k, v|
      if $v == null {{
        hide_env $k
      }} else {{
        load-env {{ ($k): $v }}
      }}
    }}

    let path_list = $env.__ORIG_PATH | split row (char esep)

    $data | get paths | reverse | each {{ |p|
      let path_list = ($path_list | prepend $p)
    }}

    $env.{path_name} = ($path_list | uniq)
  }}
}})"#
                )
            }
        }))
    }

    fn get_config_path(&self, home_dir: &Path) -> PathBuf {
        get_config_dir(home_dir).join("nushell").join("config.nu")
    }

    fn get_env_path(&self, home_dir: &Path) -> PathBuf {
        get_config_dir(home_dir).join("nushell").join("env.nu")
    }

    // https://www.nushell.sh/book/configuration.html
    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf> {
        HashSet::<PathBuf>::from_iter([
            get_config_dir(home_dir).join("nushell").join("env.nu"),
            home_dir.join(".config").join("nushell").join("env.nu"),
            get_config_dir(home_dir).join("nushell").join("config.nu"),
            home_dir.join(".config").join("nushell").join("config.nu"),
        ])
        .into_iter()
        .collect()
    }

    /// Quotes a string according to Nu shell quoting rules.
    /// @see <https://www.nushell.sh/book/working_with_strings.html>
    fn quote(&self, input: &str) -> String {
        if input.contains('`') {
            // Use backtick quoting for strings containing backticks
            format!("`{}`", input)
        } else if input.contains('\'') {
            // Use double quotes with proper escaping for single-quoted strings
            format!(
                "\"{}\"",
                input
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('\n', "\\n")
            )
        } else if input.contains('"') {
            // Escape double quotes if present
            format!(
                "\"{}\"",
                input
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('\n', "\\n")
            )
        } else {
            // Use single quotes for other cases
            format!("'{}'", input.replace('\n', "\\n"))
        }
    }
}

impl fmt::Display for Nu {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "nu")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_env_var() {
        assert_eq!(
            Nu.format_env_set("PROTO_HOME", "$HOME/.proto"),
            r#"$env.PROTO_HOME = ($env.HOME | path join '.proto')"#
        );
    }

    #[cfg(unix)]
    #[test]
    fn formats_path() {
        assert_eq!(
            Nu.format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"$env.PATH = ($env.PATH | split row (char esep)
  | prepend ($env.PROTO_HOME | path join bin)
  | prepend ($env.PROTO_HOME | path join shims)
  | uniq)"#
        );

        assert_eq!(
            Nu.format_path_set(&["$HOME/with/sub/dir".into(), "/some/abs/path/bin".into()]),
            r#"$env.PATH = ($env.PATH | split row (char esep)
  | prepend /some/abs/path/bin
  | prepend ($env.HOME | path join with sub dir)
  | uniq)"#
        );
    }

    #[cfg(windows)]
    #[test]
    fn formats_path() {
        assert_eq!(
            Nu.format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()])
                .replace("\r\n", "\n"),
            r#"$env.Path = ($env.Path | split row (char esep)
  | prepend ($env.PROTO_HOME | path join bin)
  | prepend ($env.PROTO_HOME | path join shims)
  | uniq)"#
        );

        assert_eq!(
            Nu.format_path_set(&["$HOME/with/sub/dir".into(), "/some/abs/path/bin".into()])
                .replace("\r\n", "\n"),
            r#"$env.Path = ($env.Path | split row (char esep)
  | prepend /some/abs/path/bin
  | prepend ($env.HOME | path join with sub dir)
  | uniq)"#
        );
    }

    #[cfg(unix)]
    #[test]
    fn formats_cd_hook() {
        use starbase_sandbox::assert_snapshot;

        let hook = Hook::OnChangeDir {
            command: "starbase hook nu".into(),
            prefix: "starbase".into(),
        };

        assert_snapshot!(Nu.format_hook(hook).unwrap());
    }

    #[test]
    fn test_nu_quoting() {
        assert_eq!(Nu.quote("hello"), "'hello'");
        assert_eq!(Nu.quote(""), "''");
        assert_eq!(Nu.quote("echo 'hello'"), "\"echo 'hello'\"");
        assert_eq!(Nu.quote("echo \"$HOME\""), "\"echo \\\"$HOME\\\"\"");
        assert_eq!(Nu.quote("\"hello\""), "\"\\\"hello\\\"\"");
        assert_eq!(Nu.quote("\"hello\nworld\""), "\"\\\"hello\\nworld\\\"\"");
        assert_eq!(Nu.quote("$'hello world'"), "\"$'hello world'\"");
        assert_eq!(Nu.quote("$''"), "\"$''\"");
        assert_eq!(Nu.quote("$\"hello world\""), "\"$\\\"hello world\\\"\"");
        assert_eq!(Nu.quote("$\"$HOME\""), "\"$\\\"$HOME\\\"\"");
        assert_eq!(Nu.quote("'hello'"), "\"'hello'\"");
    }
}
