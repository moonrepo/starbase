use super::{Shell, ShellCommand};
use crate::helpers::{
    ProfileSet, get_env_key_native, get_env_var_regex, normalize_newlines, quotable_into_string,
};
use crate::hooks::*;
use crate::quoter::*;
use shell_quote::Quotable;
use std::env;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub struct PowerShell;

impl PowerShell {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self
    }

    // $FOO -> $env:FOO
    fn replace_env(&self, value: impl AsRef<str>) -> String {
        get_env_var_regex()
            .replace_all(value.as_ref(), "$$env:$name")
            // https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_automatic_variables?view=powershell-5.1#home
            .replace("$env:HOME", "$HOME")
    }

    fn join_path(&self, value: impl AsRef<str>) -> String {
        let value = value.as_ref();

        // When no variable, return as-is
        if !value.contains('$') {
            return format!("\"{value}\"");
        }

        // Otherwise split into segments and join
        let parts = self
            .replace_env(value)
            .split(['/', '\\'])
            .map(|part| {
                if part.starts_with('$') {
                    part.to_owned()
                } else {
                    format!("\"{part}\"")
                }
            })
            .collect::<Vec<_>>();

        if parts.len() == 1 {
            return parts.join("");
        }

        format!("Join-Path {}", parts.join(" "))
    }

    /// Quotes a string according to PowerShell shell quoting rules.
    /// @see <https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_quoting_rules>
    fn do_quote(value: String) -> String {
        // Check if the string contains any characters that need to be escaped
        if value.contains('\'') || value.contains('"') || value.contains('`') || value.contains('$')
        {
            // If the string contains a single quote, use a single-quoted string and escape single quotes by doubling them
            if value.contains('\'') {
                let escaped = value.replace('\'', "''");

                return format!("'{escaped}'");
            } else {
                // Use a double-quoted string and escape necessary characters
                let escaped = value.replace('`', "``").replace('"', "`\"");

                return format!("\"{escaped}\"");
            }
        }

        // If the string does not contain any special characters, return a single-quoted string
        format!("'{value}'")
    }

    fn do_quote_expansion(value: String) -> String {
        let mut output = String::with_capacity(value.len() + 2);
        output.push('"');

        for c in value.chars() {
            if c == '"' {
                output.push('"');
            }
            output.push(c);
        }

        output.push('"');
        output
    }
}

// https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_profiles?view=powershell-5.1
impl Shell for PowerShell {
    fn create_quoter<'a>(&self, data: Quotable<'a>) -> Quoter<'a> {
        Quoter::new(
            data,
            QuoterOptions {
                quoted_syntax: vec![
                    Syntax::Pair("$(".into(), ")".into()),
                    Syntax::Pair("${".into(), "}".into()),
                ],
                on_quote: Arc::new(|data| PowerShell::do_quote(quotable_into_string(data))),
                on_quote_expansion: Arc::new(|data| {
                    PowerShell::do_quote_expansion(quotable_into_string(data))
                }),
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
                let mut value = format!("$env:{} = @(\n", get_env_key_native(key));

                for path in paths {
                    let path = self.join_path(path);

                    if path.starts_with("Join-Path") {
                        value.push_str(&format!("  ({path})\n"));
                    } else {
                        value.push_str(&format!("  {path}\n"));
                    }
                }

                if let Some(orig_key) = orig_key {
                    value.push_str("  $env:");
                    value.push_str(get_env_key_native(orig_key));
                    value.push('\n');
                }

                value.push_str(") -join [IO.PATH]::PathSeparator;");

                normalize_newlines(value)
            }
            Statement::SetEnv { key, value } => {
                let key = get_env_key_native(key);

                if value.contains('/') || value.contains('\\') {
                    format!("$env:{} = {};", key, self.join_path(value))
                } else {
                    format!(
                        "$env:{} = {};",
                        key,
                        self.quote(self.replace_env(value).as_str())
                    )
                }
            }
            Statement::UnsetEnv { key } => {
                format!(
                    r#"if (Test-Path "env:{}") {{
  Remove-Item -LiteralPath "env:{key}";
}}"#,
                    get_env_key_native(key)
                )
            }
        }
    }

    fn get_config_path(&self, home_dir: &Path) -> PathBuf {
        home_dir
            .join("Documents")
            .join("PowerShell")
            .join("Microsoft.PowerShell_profile.ps1")
    }

    fn get_env_path(&self, home_dir: &Path) -> PathBuf {
        self.get_config_path(home_dir)
    }

    fn get_env_regex(&self) -> regex::Regex {
        regex::Regex::new(r"\$(Env|env):(?<name>[A-Za-z0-9_]+)").unwrap()
    }

    fn get_exec_command(&self) -> ShellCommand {
        ShellCommand {
            shell_args: vec!["-NoLogo".into(), "-c".into()],
            pass_args_stdin: false,
        }
    }

    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf> {
        let mut profiles = ProfileSet::default();

        // https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_profiles?view=powershell-5.1#the-profile-variable
        if let Some(profile) = env::var_os("PROFILE") {
            profiles = profiles.insert(PathBuf::from(profile), 10);
        }

        let docs_dir = home_dir.join("Documents");

        profiles = profiles
            .insert(docs_dir.join("WindowsPowerShell").join("Profile.ps1"), 1)
            .insert(
                docs_dir
                    .join("WindowsPowerShell")
                    .join("Microsoft.PowerShell_profile.ps1"),
                2,
            );

        profiles.into_list()
    }
}

impl fmt::Display for PowerShell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "powershell")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_env_var() {
        assert_eq!(
            PowerShell.format_env_set("PROTO_HOME", "$HOME/.proto"),
            r#"$env:PROTO_HOME = Join-Path $HOME ".proto";"#
        );
        assert_eq!(
            PowerShell.format_env_set("PROTO_HOME", "$HOME"),
            r#"$env:PROTO_HOME = "$HOME";"#
        );
        assert_eq!(
            PowerShell.format_env_set("BOOL", "true"),
            r#"$env:BOOL = 'true';"#
        );
        assert_eq!(
            PowerShell.format_env_set("STRING", "a b c"),
            r#"$env:STRING = 'a b c';"#
        );
    }

    #[cfg(unix)]
    #[test]
    fn formats_path_prepend() {
        assert_eq!(
            PowerShell
                .format_path_prepend(&["$PROTO_HOME/shims".into(), "$PROTO_HOME\\bin".into()])
                .replace("\r\n", "\n"),
            r#"$env:PATH = @(
  (Join-Path $env:PROTO_HOME "shims")
  (Join-Path $env:PROTO_HOME "bin")
  $env:PATH
) -join [IO.PATH]::PathSeparator;"#
        );

        assert_eq!(
            PowerShell
                .format_path_prepend(&["$HOME".into()])
                .replace("\r\n", "\n"),
            r#"$env:PATH = @(
  $HOME
  $env:PATH
) -join [IO.PATH]::PathSeparator;"#
        );

        assert_eq!(
            PowerShell
                .format_path_prepend(&["$BINPATH".into(), "C:\\absolute\\path".into()])
                .replace("\r\n", "\n"),
            r#"$env:PATH = @(
  $env:BINPATH
  "C:\absolute\path"
  $env:PATH
) -join [IO.PATH]::PathSeparator;"#
        );
    }

    #[cfg(unix)]
    #[test]
    fn formats_path_set() {
        assert_eq!(
            PowerShell
                .format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME\\bin".into()])
                .replace("\r\n", "\n"),
            r#"$env:PATH = @(
  (Join-Path $env:PROTO_HOME "shims")
  (Join-Path $env:PROTO_HOME "bin")
) -join [IO.PATH]::PathSeparator;"#
        );

        assert_eq!(
            PowerShell
                .format_path_set(&["$HOME".into()])
                .replace("\r\n", "\n"),
            r#"$env:PATH = @(
  $HOME
) -join [IO.PATH]::PathSeparator;"#
        );

        assert_eq!(
            PowerShell
                .format_path_set(&["$BINPATH".into(), "C:\\absolute\\path".into()])
                .replace("\r\n", "\n"),
            r#"$env:PATH = @(
  $env:BINPATH
  "C:\absolute\path"
) -join [IO.PATH]::PathSeparator;"#
        );
    }

    #[cfg(windows)]
    #[test]
    fn formats_path_prepend() {
        assert_eq!(
            PowerShell
                .format_path_prepend(&["$PROTO_HOME/shims".into(), "$PROTO_HOME\\bin".into()])
                .replace("\r\n", "\n"),
            r#"$env:Path = @(
  (Join-Path $env:PROTO_HOME "shims")
  (Join-Path $env:PROTO_HOME "bin")
  $env:Path
) -join [IO.PATH]::PathSeparator;"#
        );

        assert_eq!(
            PowerShell
                .format_path_prepend(&["$HOME".into()])
                .replace("\r\n", "\n"),
            r#"$env:Path = @(
  $HOME
  $env:Path
) -join [IO.PATH]::PathSeparator;"#
        );

        assert_eq!(
            PowerShell
                .format_path_prepend(&["$BINPATH".into(), "C:\\absolute\\path".into()])
                .replace("\r\n", "\n"),
            r#"$env:Path = @(
  $env:BINPATH
  "C:\absolute\path"
  $env:Path
) -join [IO.PATH]::PathSeparator;"#
        );
    }

    #[cfg(windows)]
    #[test]
    fn formats_path_set() {
        assert_eq!(
            PowerShell
                .format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME\\bin".into()])
                .replace("\r\n", "\n"),
            r#"$env:Path = @(
  (Join-Path $env:PROTO_HOME "shims")
  (Join-Path $env:PROTO_HOME "bin")
) -join [IO.PATH]::PathSeparator;"#
        );

        assert_eq!(
            PowerShell
                .format_path_set(&["$HOME".into()])
                .replace("\r\n", "\n"),
            r#"$env:Path = @(
  $HOME
) -join [IO.PATH]::PathSeparator;"#
        );

        assert_eq!(
            PowerShell
                .format_path_set(&["$BINPATH".into(), "C:\\absolute\\path".into()])
                .replace("\r\n", "\n"),
            r#"$env:Path = @(
  $env:BINPATH
  "C:\absolute\path"
) -join [IO.PATH]::PathSeparator;"#
        );
    }

    #[test]
    fn test_profile_paths() {
        #[allow(deprecated)]
        let home_dir = std::env::home_dir().unwrap();

        assert_eq!(
            PowerShell::new().get_profile_paths(&home_dir),
            vec![
                home_dir
                    .join("Documents")
                    .join("WindowsPowerShell")
                    .join("Profile.ps1"),
                home_dir
                    .join("Documents")
                    .join("WindowsPowerShell")
                    .join("Microsoft.PowerShell_profile.ps1"),
            ]
        );
    }

    #[test]
    fn test_pwsh_quoting() {
        assert_eq!(PowerShell.quote(""), "''");
        assert_eq!(PowerShell.quote("simple"), "'simple'");
        assert_eq!(PowerShell.quote("don't"), "'don''t'");
        assert_eq!(PowerShell.quote("say \"hello\""), "\"say `\"hello`\"\"");
        assert_eq!(PowerShell.quote("back`tick"), "\"back``tick\"");
        // assert_eq!(PowerShell.quote("price $5"), "\"price `$5\"");
    }
}
