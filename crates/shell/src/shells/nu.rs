use super::Shell;
use crate::helpers::{
    ProfileSet, get_config_dir, get_env_key_native, get_env_var_regex, normalize_newlines,
};
use crate::hooks::*;
use crate::quoter::*;
use shell_quote::Quotable;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Nu;

impl Nu {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self
    }

    fn join_path(&self, value: impl AsRef<str>) -> Option<String> {
        let parts = value
            .as_ref()
            .split(['/', '\\'])
            .filter(|part| !part.is_empty())
            .map(Self::quote_string)
            .collect::<Vec<_>>();

        if parts.is_empty() {
            None
        } else {
            Some(format!("path join {}", parts.join(" ")))
        }
    }

    fn quote_string(value: impl AsRef<str>) -> String {
        let value = value.as_ref();
        let mut output = String::with_capacity(value.len() + 2);

        output.push('"');

        for ch in value.chars() {
            match ch {
                '\0' => output.push_str("\\u{0}"),
                '\t' => output.push_str("\\t"),
                '\n' => output.push_str("\\n"),
                '\r' => output.push_str("\\r"),
                '"' => output.push_str("\\\""),
                '\\' => output.push_str("\\\\"),
                _ => output.push(ch),
            }
        }

        output.push('"');
        output
    }
}

impl Shell for Nu {
    fn create_quoter<'a>(&self, data: Quotable<'a>) -> Quoter<'a> {
        let mut options = QuoterOptions::default();

        // https://www.nushell.sh/book/working_with_strings.html
        options.quote_pairs.clear();
        options.quote_pairs.push(("'".into(), "'".into(), false));
        options.quote_pairs.push(("\"".into(), "\"".into(), false));
        options.quote_pairs.push(("r#".into(), "#".into(), false));
        options.quote_pairs.push(("`".into(), "`".into(), false));
        options.quote_pairs.push(("$'".into(), "'".into(), false));
        options.quote_pairs.push(("$\"".into(), "\"".into(), true));

        // https://www.nushell.sh/book/working_with_strings.html#double-quoted-strings
        options.replacements_expansion.insert('\0', "\\u{0}");

        Quoter::new(data, options)
    }

    // https://www.nushell.sh/book/configuration.html#environment
    fn format(&self, statement: Statement<'_>) -> String {
        match statement {
            Statement::ModifyPath {
                paths,
                key,
                orig_key,
            } => {
                // $FOO -> $env.FOO
                let env_regex = get_env_var_regex();
                let key = key.unwrap_or("PATH");

                let mut value = match orig_key {
                    Some(orig_key) => format!(
                        "$env.{} = ($env.{} | split row (char esep)\n",
                        get_env_key_native(key),
                        get_env_key_native(orig_key)
                    ),
                    None => format!("$env.{} = ([]\n", get_env_key_native(key),),
                };

                // https://www.nushell.sh/book/configuration.html#path-configuration
                for path in paths.iter().rev() {
                    value.push_str("  | prepend ");

                    match env_regex
                        .captures(path)
                        .filter(|cap| cap.get(0).is_some_and(|env_match| env_match.start() == 0))
                    {
                        Some(cap) => {
                            let env_match = cap.get(0).unwrap();
                            let path_without_env = &path[env_match.end()..];

                            value.push('(');
                            value.push_str(&format!("$env.{}", cap.name("name").unwrap().as_str()));

                            if let Some(join_path) = self.join_path(path_without_env) {
                                value.push_str(" | ");
                                value.push_str(&join_path);
                            }

                            value.push(')');
                        }
                        _ => {
                            value.push_str(&Self::quote_string(path));
                        }
                    }

                    value.push('\n');
                }

                value.push_str("  | uniq)");

                normalize_newlines(value)
            }
            Statement::SetAlias { name, value } => {
                // A Nushell alias maps a name to an expression (a command line), not
                // to a string. Quoting the value (e.g. `alias ll = 'ls -la'`) would
                // alias to the string literal instead of running the command.
                format!("alias {name} = {value}")
            }
            Statement::SetEnv { key, value } => {
                if value.starts_with("$HOME/") {
                    let path = value.trim_start_matches("$HOME/");
                    format!(
                        "$env.{} = ($env.{} | path join {})",
                        get_env_key_native(key),
                        get_env_key_native("HOME"),
                        Self::quote_string(path)
                    )
                } else {
                    format!("$env.{} = {}", get_env_key_native(key), self.quote(value))
                }
            }
            Statement::UnsetAlias { name } => {
                format!("hide {name}")
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
            Hook::OnChangeDir {
                activate_command,
                activate_function,
                deactivate_command,
                deactivate_function,
            } => {
                format!(
                    r#"
export def --env {activate_function}_apply [data] {{
    # This must be a `for` loop and not an `each`/`items` closure,
    # as closures do not propagate environment changes, even when
    # the command itself is `def --env`.
    for pair in ($data | get --optional env | default {{}} | transpose key value) {{
        if $pair.value == null {{
            if $pair.key in $env {{
                hide-env $pair.key
            }}
        }} else {{
            load-env {{ ($pair.key): $pair.value }}
        }}
    }}

    let path_list = $data | get --optional paths | default []
    let path_string = $data | get --optional path | default ''

    if ($path_list | is-not-empty) {{
        $env.{path_key} = $path_list
    }}

    if ($path_string | is-not-empty) {{
        $env.{path_key} = $path_string
    }}
}}

export def --env {activate_function} [] {{
    {activate_function}_apply ({activate_command} | from json)
}}

export def --env {deactivate_function} [] {{
    # Unregistering must happen even when the command fails, so the
    # reversal is best-effort, consistent with the other shells.
    {activate_function}_apply (try {{ {deactivate_command} | from json }} catch {{ {{}} }})

    # Nu cannot undefine commands at runtime, so the functions remain
    # defined, but the hook itself is unregistered.
    $env.config = ($env.config | upsert hooks.env_change.PWD (
        ($env.config | get --optional hooks.env_change.PWD) | default []
            | where {{ |hook| $hook != {{ code: "{activate_function}" }} }}
    ))
}}

export-env {{
    $env.config = ($env.config | upsert hooks.env_change.PWD {{ |config|
        let list = ($config | get --optional hooks.env_change.PWD) | default []
        let hook = {{ code: "{activate_function}" }}

        if $hook in $list {{
            $list
        }} else {{
            $list | append $hook
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

    fn get_env_regex(&self) -> regex::Regex {
        regex::Regex::new(r"\$env.(?<name>[A-Za-z0-9_]+)").unwrap()
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
            r#"$env.PROTO_HOME = ($env.HOME | path join ".proto")"#
        );
        assert_eq!(Nu.format_env_set("FOO", "don't"), r#"$env.FOO = "don't""#);
    }

    #[cfg(windows)]
    #[test]
    fn formats_env_var() {
        assert_eq!(
            Nu.format_env_set("PROTO_HOME", "$HOME/.proto"),
            r#"$env.PROTO_HOME = ($env.USERPROFILE | path join ".proto")"#
        );
        assert_eq!(Nu.format_env_set("FOO", "don't"), r#"$env.FOO = "don't""#);
    }

    #[cfg(unix)]
    #[test]
    fn formats_path_prepend() {
        assert_eq!(
            Nu.format_path_prepend(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"$env.PATH = ($env.PATH | split row (char esep)
  | prepend ($env.PROTO_HOME | path join "bin")
  | prepend ($env.PROTO_HOME | path join "shims")
  | uniq)"#
        );

        assert_eq!(
            Nu.format_path_prepend(&["$HOME/with/sub/dir".into(), "/some/abs/path/bin".into()]),
            r#"$env.PATH = ($env.PATH | split row (char esep)
  | prepend "/some/abs/path/bin"
  | prepend ($env.HOME | path join "with" "sub" "dir")
  | uniq)"#
        );
    }

    #[cfg(unix)]
    #[test]
    fn formats_path_set() {
        assert_eq!(
            Nu.format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"$env.PATH = ([]
  | prepend ($env.PROTO_HOME | path join "bin")
  | prepend ($env.PROTO_HOME | path join "shims")
  | uniq)"#
        );

        assert_eq!(
            Nu.format_path_set(&["$HOME/with/sub/dir".into(), "/some/abs/path/bin".into()]),
            r#"$env.PATH = ([]
  | prepend "/some/abs/path/bin"
  | prepend ($env.HOME | path join "with" "sub" "dir")
  | uniq)"#
        );
    }

    #[cfg(windows)]
    #[test]
    fn formats_path_prepend() {
        assert_eq!(
            Nu.format_path_prepend(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()])
                .replace("\r\n", "\n"),
            r#"$env.Path = ($env.Path | split row (char esep)
  | prepend ($env.PROTO_HOME | path join "bin")
  | prepend ($env.PROTO_HOME | path join "shims")
  | uniq)"#
        );

        assert_eq!(
            Nu.format_path_prepend(&["$HOME/with/sub/dir".into(), "/some/abs/path/bin".into()])
                .replace("\r\n", "\n"),
            r#"$env.Path = ($env.Path | split row (char esep)
  | prepend "/some/abs/path/bin"
  | prepend ($env.HOME | path join "with" "sub" "dir")
  | uniq)"#
        );
    }

    #[cfg(windows)]
    #[test]
    fn formats_path_set() {
        assert_eq!(
            Nu.format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()])
                .replace("\r\n", "\n"),
            r#"$env.Path = ([]
  | prepend ($env.PROTO_HOME | path join "bin")
  | prepend ($env.PROTO_HOME | path join "shims")
  | uniq)"#
        );

        assert_eq!(
            Nu.format_path_set(&["$HOME/with/sub/dir".into(), "/some/abs/path/bin".into()])
                .replace("\r\n", "\n"),
            r#"$env.Path = ([]
  | prepend "/some/abs/path/bin"
  | prepend ($env.HOME | path join "with" "sub" "dir")
  | uniq)"#
        );
    }

    #[cfg(unix)]
    #[test]
    fn quotes_paths_with_spaces() {
        assert_eq!(
            Nu.format_path_set(&["/tmp/a b".into()])
                .replace("\r\n", "\n"),
            r#"$env.PATH = ([]
  | prepend "/tmp/a b"
  | uniq)"#
        );
        assert_eq!(
            Nu.format_path_set(&["$HOME/a b/bin".into()])
                .replace("\r\n", "\n"),
            r#"$env.PATH = ([]
  | prepend ($env.HOME | path join "a b" "bin")
  | uniq)"#
        );
        assert_eq!(
            Nu.format_path_set(&["$HOME".into()]).replace("\r\n", "\n"),
            r#"$env.PATH = ([]
  | prepend ($env.HOME)
  | uniq)"#
        );
    }

    #[cfg(windows)]
    #[test]
    fn quotes_paths_with_spaces() {
        assert_eq!(
            Nu.format_path_set(&["/tmp/a b".into()])
                .replace("\r\n", "\n"),
            r#"$env.Path = ([]
  | prepend "/tmp/a b"
  | uniq)"#
        );
        assert_eq!(
            Nu.format_path_set(&["$HOME/a b/bin".into()])
                .replace("\r\n", "\n"),
            r#"$env.Path = ([]
  | prepend ($env.HOME | path join "a b" "bin")
  | uniq)"#
        );
        assert_eq!(
            Nu.format_path_set(&["$HOME".into()]).replace("\r\n", "\n"),
            r#"$env.Path = ([]
  | prepend ($env.HOME)
  | uniq)"#
        );
    }

    #[cfg(unix)]
    #[test]
    fn formats_cd_hook() {
        use starbase_sandbox::assert_snapshot;

        let hook = Hook::OnChangeDir {
            activate_command: "starbase hook nu".into(),
            activate_function: "_starbase_hook".into(),
            deactivate_command: "starbase deactivate nu".into(),
            deactivate_function: "_starbase_deactivate".into(),
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
    fn formats_alias_set() {
        assert_eq!(Nu.format_alias_set("ll", "ls -la"), "alias ll = ls -la");
    }

    #[test]
    fn formats_alias_unset() {
        assert_eq!(Nu.format_alias_unset("ll"), "hide ll");
    }

    #[test]
    fn test_nu_quoting() {
        assert_eq!(Nu.quote("hello"), "hello");
        assert_eq!(Nu.quote(""), "''");
        assert_eq!(Nu.quote("echo 'hello'"), "\"echo 'hello'\"");
        assert_eq!(Nu.quote("echo \"$HOME\""), "$\"echo \\\"$HOME\\\"\"");
        assert_eq!(Nu.quote("\"hello\""), "\"hello\"");
        assert_eq!(Nu.quote("\"hello\nworld\""), "\"hello\nworld\"");
        assert_eq!(Nu.quote("$'hello world'"), "$'hello world'");
        assert_eq!(Nu.quote("$''"), "$''");
        assert_eq!(Nu.quote("$\"hello world\""), "$\"hello world\"");
        assert_eq!(Nu.quote("$\"$HOME\""), "$\"$HOME\"");
        assert_eq!(Nu.quote("'hello'"), "'hello'");
    }
}
