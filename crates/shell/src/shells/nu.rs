use super::Shell;
use crate::helpers::{
    ProfileSet, get_config_dir, get_env_key_native, get_env_var_regex, normalize_newlines,
};
use crate::hooks::*;
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
        match statement {
            Statement::PrependPath {
                paths,
                key,
                orig_key,
            } => {
                let env_regex = get_env_var_regex();
                let key = key.unwrap_or("PATH");
                let orig_key = orig_key.unwrap_or(key);

                let mut value = format!(
                    "$env.{} = ($env.{} | split row (char esep)\n",
                    get_env_key_native(key),
                    get_env_key_native(orig_key)
                );

                // https://www.nushell.sh/book/configuration.html#path-configuration
                for path in paths.iter().rev() {
                    value.push_str("  | prepend ");

                    match env_regex.captures(path) {
                        Some(cap) => {
                            let path_without_env = path.replace(cap.get(0).unwrap().as_str(), "");

                            value.push('(');
                            value.push_str(&format!("$env.{}", cap.name("name").unwrap().as_str()));
                            value.push_str(" | ");
                            value.push_str(&join_path(path_without_env));
                            value.push(')');
                        }
                        _ => {
                            value.push_str(path);
                        }
                    }

                    value.push('\n');
                }

                value.push_str("  | uniq)");

                normalize_newlines(value)
            }
            Statement::SetEnv { key, value } => {
                if value.starts_with("$HOME/") {
                    let path = value.trim_start_matches("$HOME/");
                    format!(
                        "$env.{} = ($env.{} | path join '{}')",
                        get_env_key_native(key),
                        get_env_key_native("HOME"),
                        path
                    )
                } else {
                    format!("$env.{} = {}", get_env_key_native(key), self.quote(value))
                }
            }
            Statement::UnsetEnv { key } => {
                format!("hide-env {}", get_env_key_native(key))
            }
        }
    }

    fn format_hook(&self, hook: Hook) -> Result<String, crate::ShellError> {
        let path_key = get_env_key_native("PATH");

        // https://www.nushell.sh/book/hooks.html#adding-a-single-hook-to-existing-config
        Ok(normalize_newlines(match hook {
            Hook::OnChangeDir { command, function } => {
                format!(
                    r#"
export def {function} [] {{
    let data = {command} | from json

    $data | get -i env | items {{ |k, v|
        if $v == null {{
            if $k in $env {{
                hide-env $k
            }}
        }} else {{
            load-env {{ ($k): $v }}
        }}
    }}

    let path_list = $data | get -i paths | default []
    let path_string = $data | get -i path | default ''

    if ($path_list | is-not-empty) {{
        $env.{path_key} = $path_list
    }}

    if ($path_string | is-not-empty) {{
        $env.{path_key} = $path_string
    }}
}}

export-env {{
    $env.__ORIG_PATH = $env.{path_key}

    $env.config = ($env.config | upsert hooks.env_change.PWD {{ |config|
        let list = ($config | get -i hooks.env_change.PWD) | default []

        $list | append {{ |before, after|
            {function}
        }}
    }})
}}"#
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
        let mut profiles = ProfileSet::default();
        let mut order = 0;
        let mut inc = || {
            order += 1;
            order
        };

        for name in ["config.nu", "env.nu"] {
            #[cfg(windows)]
            {
                profiles = profiles.insert(
                    home_dir
                        .join("AppData")
                        .join("Roaming")
                        .join("nushell")
                        .join(name),
                    inc(),
                );
            }

            profiles = profiles
                .insert(get_config_dir(home_dir).join("nushell").join(name), inc())
                .insert(home_dir.join(".config").join("nushell").join(name), inc());
        }

        profiles.into_list()
    }

    /// Quotes a string according to Nu shell quoting rules.
    /// @see <https://www.nushell.sh/book/working_with_strings.html>
    fn quote(&self, input: &str) -> String {
        if self.requires_expansion(input) {
            if input.starts_with("$\"") {
                return input.into();
            } else {
                return format!("$\"{input}\"");
            }
        }

        if input.contains('`') {
            // Use backtick quoting for strings containing backticks
            format!("`{input}`")
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

    #[cfg(unix)]
    #[test]
    fn formats_env_var() {
        assert_eq!(
            Nu.format_env_set("PROTO_HOME", "$HOME/.proto"),
            r#"$env.PROTO_HOME = ($env.HOME | path join '.proto')"#
        );
    }

    #[cfg(windows)]
    #[test]
    fn formats_env_var() {
        assert_eq!(
            Nu.format_env_set("PROTO_HOME", "$HOME/.proto"),
            r#"$env.PROTO_HOME = ($env.USERPROFILE | path join '.proto')"#
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
            function: "_starbase_hook".into(),
        };

        assert_snapshot!(Nu.format_hook(hook).unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn test_profile_paths() {
        #[allow(deprecated)]
        let home_dir = std::env::home_dir().unwrap();

        assert_eq!(
            Nu::new().get_profile_paths(&home_dir),
            vec![
                home_dir.join(".config").join("nushell").join("config.nu"),
                home_dir.join(".config").join("nushell").join("env.nu"),
            ]
        );
    }

    #[cfg(windows)]
    #[test]
    fn test_profile_paths() {
        #[allow(deprecated)]
        let home_dir = std::env::home_dir().unwrap();

        assert_eq!(
            Nu::new().get_profile_paths(&home_dir),
            vec![
                home_dir
                    .join("AppData")
                    .join("Roaming")
                    .join("nushell")
                    .join("config.nu"),
                home_dir.join(".config").join("nushell").join("config.nu"),
                home_dir
                    .join("AppData")
                    .join("Roaming")
                    .join("nushell")
                    .join("env.nu"),
                home_dir.join(".config").join("nushell").join("env.nu"),
            ]
        );
    }

    #[test]
    fn test_nu_quoting() {
        assert_eq!(Nu.quote("hello"), "'hello'");
        assert_eq!(Nu.quote(""), "''");
        assert_eq!(Nu.quote("echo 'hello'"), "\"echo 'hello'\"");
        assert_eq!(Nu.quote("echo \"$HOME\""), "$\"echo \"$HOME\"\"");
        assert_eq!(Nu.quote("\"hello\""), "\"\\\"hello\\\"\"");
        assert_eq!(Nu.quote("\"hello\nworld\""), "\"\\\"hello\\nworld\\\"\"");
        assert_eq!(Nu.quote("$'hello world'"), "\"$'hello world'\"");
        assert_eq!(Nu.quote("$''"), "\"$''\"");
        assert_eq!(Nu.quote("$\"hello world\""), "\"$\\\"hello world\\\"\"");
        assert_eq!(Nu.quote("$\"$HOME\""), "$\"$HOME\"");
        assert_eq!(Nu.quote("'hello'"), "\"'hello'\"");
    }
}
