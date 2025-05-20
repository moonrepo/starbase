use super::{Shell, ShellCommand};
use crate::helpers::{ProfileSet, get_env_key_native, get_env_var_regex, normalize_newlines};
use crate::hooks::*;
use std::env;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Pwsh;

impl Pwsh {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self
    }

    fn format_env(&self, value: impl AsRef<str>) -> String {
        get_env_var_regex()
            .replace_all(value.as_ref(), "$$env:$name")
            // https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_automatic_variables?view=powershell-7.4#home
            .replace("$env:HOME", "$HOME")
    }

    fn join_path(&self, value: impl AsRef<str>) -> String {
        let value = value.as_ref();

        // When no variable, return as-is
        if !value.contains('$') {
            return format!("\"{}\"", value);
        }

        // Otherwise split into segments and join
        let parts = self
            .format_env(value)
            .split(['/', '\\'])
            .map(|part| {
                if part.starts_with('$') {
                    part.to_owned()
                } else {
                    format!("\"{}\"", part)
                }
            })
            .collect::<Vec<_>>();

        if parts.len() == 1 {
            return parts.join("");
        }

        format!("Join-Path {}", parts.join(" "))
    }
}

// https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_profiles?view=powershell-7.4
impl Shell for Pwsh {
    fn format(&self, statement: Statement<'_>) -> String {
        match statement {
            Statement::PrependPath {
                paths,
                key,
                orig_key,
            } => {
                let key = key.unwrap_or("PATH");
                let orig_key = orig_key.unwrap_or(key);
                let mut value = format!("$env:{} = @(\n", get_env_key_native(key));

                for path in paths {
                    let path = self.join_path(path);

                    if path.starts_with("Join-Path") {
                        value.push_str(&format!("  ({})\n", path));
                    } else {
                        value.push_str(&format!("  {}\n", path));
                    }
                }

                value.push_str("  $env:");
                value.push_str(orig_key);
                value.push_str("\n) -join [IO.PATH]::PathSeparator;");

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
                        self.quote(self.format_env(value).as_str())
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

    fn format_hook(&self, hook: Hook) -> Result<String, crate::ShellError> {
        Ok(normalize_newlines(match hook {
            Hook::OnChangeDir { command, function } => {
                format!(
                    r#"using namespace System;
using namespace System.Management.Automation;

$origPath = [Environment]::GetEnvironmentVariable('PATH')
[Environment]::SetEnvironmentVariable('__ORIG_PATH', "$origPath");

function {function} {{
  $exports = {command};
  if ($exports) {{
    $exports | Out-String | Invoke-Expression;
  }}
}}

$hook = [EventHandler[LocationChangedEventArgs]] {{
  param([object] $source, [LocationChangedEventArgs] $changedArgs)
  end {{
    {function}
  }}
}};

$currentAction = $ExecutionContext.SessionState.InvokeCommand.LocationChangedAction;

if ($currentAction) {{
  $ExecutionContext.SessionState.InvokeCommand.LocationChangedAction = [Delegate]::Combine($currentAction, $hook);
}} else {{
  $ExecutionContext.SessionState.InvokeCommand.LocationChangedAction = $hook;
}};
"#
                )
            }
        }))
    }

    fn get_config_path(&self, home_dir: &Path) -> PathBuf {
        #[cfg(windows)]
        {
            home_dir
                .join("Documents")
                .join("PowerShell")
                .join("Microsoft.PowerShell_profile.ps1")
        }

        #[cfg(unix)]
        {
            use crate::helpers::get_config_dir;

            get_config_dir(home_dir)
                .join("powershell")
                .join("Microsoft.PowerShell_profile.ps1")
        }
    }

    fn get_env_path(&self, home_dir: &Path) -> PathBuf {
        self.get_config_path(home_dir)
    }

    fn get_exec_command(&self) -> ShellCommand {
        ShellCommand {
            shell_args: vec![
                "-NoLogo".into(),
                "-Command".into(),
                // We'll pass the command args via stdin, so that paths with special
                // characters and spaces resolve correctly.
                // https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_pwsh?view=powershell-7.2#-command---c
                "-".into(),
            ],
            pass_args_stdin: true,
        }
    }

    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf> {
        let mut profiles = ProfileSet::default();

        // https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_automatic_variables?view=powershell-7.4#profile
        if let Some(profile) = env::var_os("PROFILE") {
            profiles = profiles.insert(PathBuf::from(profile), 10);
        }

        #[cfg(windows)]
        {
            let docs_dir = home_dir.join("Documents");

            profiles = profiles
                .insert(docs_dir.join("PowerShell").join("Profile.ps1"), 1)
                .insert(
                    docs_dir
                        .join("PowerShell")
                        .join("Microsoft.PowerShell_profile.ps1"),
                    2,
                );
        }

        #[cfg(unix)]
        {
            use crate::helpers::get_config_dir;

            profiles = profiles
                .insert(
                    get_config_dir(home_dir)
                        .join("powershell")
                        .join("profile.ps1"),
                    1,
                )
                .insert(
                    home_dir
                        .join(".config")
                        .join("powershell")
                        .join("profile.ps1"),
                    2,
                )
                .insert(
                    get_config_dir(home_dir)
                        .join("powershell")
                        .join("Microsoft.PowerShell_profile.ps1"),
                    3,
                )
                .insert(
                    home_dir
                        .join(".config")
                        .join("powershell")
                        .join("Microsoft.PowerShell_profile.ps1"),
                    4,
                );
        }

        profiles.into_list()
    }

    /// Quotes a string according to PowerShell shell quoting rules.
    /// @see <https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_quoting_rules>
    fn quote(&self, value: &str) -> String {
        // If the string is empty, return an empty single-quoted string
        if value.is_empty() {
            return "''".to_string();
        }

        // Check if the string contains any characters that need to be escaped
        if value.contains('\'') || value.contains('"') || value.contains('`') || value.contains('$')
        {
            // If the string contains a single quote, use a single-quoted string and escape single quotes by doubling them
            if value.contains('\'') {
                let escaped = value.replace('\'', "''");

                return format!("'{}'", escaped);
            } else {
                // Use a double-quoted string and escape necessary characters
                let escaped = value.replace('`', "``").replace('"', "`\"");

                return format!("\"{}\"", escaped);
            }
        }

        // If the string does not contain any special characters, return a single-quoted string
        format!("'{}'", value)
    }
}

impl fmt::Display for Pwsh {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "pwsh")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use starbase_sandbox::assert_snapshot;

    #[test]
    fn formats_env_var() {
        assert_eq!(
            Pwsh.format_env_set("PROTO_HOME", "$HOME/.proto"),
            r#"$env:PROTO_HOME = Join-Path $HOME ".proto";"#
        );
        assert_eq!(
            Pwsh.format_env_set("PROTO_HOME", "$HOME"),
            r#"$env:PROTO_HOME = "$HOME";"#
        );
        assert_eq!(
            Pwsh.format_env_set("BOOL", "true"),
            r#"$env:BOOL = 'true';"#
        );
        assert_eq!(
            Pwsh.format_env_set("STRING", "a b c"),
            r#"$env:STRING = 'a b c';"#
        );
    }

    #[test]
    fn formats_path() {
        assert_eq!(
            Pwsh.format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME\\bin".into()])
                .replace("\r\n", "\n"),
            r#"$env:PATH = @(
  (Join-Path $env:PROTO_HOME "shims")
  (Join-Path $env:PROTO_HOME "bin")
  $env:PATH
) -join [IO.PATH]::PathSeparator;"#
        );

        assert_eq!(
            Pwsh.format_path_set(&["$HOME".into()])
                .replace("\r\n", "\n"),
            r#"$env:PATH = @(
  $HOME
  $env:PATH
) -join [IO.PATH]::PathSeparator;"#
        );

        assert_eq!(
            Pwsh.format_path_set(&["$BINPATH".into(), "C:\\absolute\\path".into()])
                .replace("\r\n", "\n"),
            r#"$env:PATH = @(
  $env:BINPATH
  "C:\absolute\path"
  $env:PATH
) -join [IO.PATH]::PathSeparator;"#
        );
    }

    #[test]
    fn formats_cd_hook() {
        let hook = Hook::OnChangeDir {
            command: "starbase hook pwsh".into(),
            function: "_starbase_hook".into(),
        };

        assert_snapshot!(Pwsh.format_hook(hook).unwrap());
    }

    #[test]
    fn test_profile_paths() {
        #[allow(deprecated)]
        let home_dir = std::env::home_dir().unwrap();

        if cfg!(windows) {
            assert_eq!(
                Pwsh::new().get_profile_paths(&home_dir),
                vec![
                    home_dir
                        .join("Documents")
                        .join("PowerShell")
                        .join("Profile.ps1"),
                    home_dir
                        .join("Documents")
                        .join("PowerShell")
                        .join("Microsoft.PowerShell_profile.ps1"),
                ]
            );
        } else {
            assert_eq!(
                Pwsh::new().get_profile_paths(&home_dir),
                vec![
                    home_dir
                        .join(".config")
                        .join("powershell")
                        .join("profile.ps1"),
                    home_dir
                        .join(".config")
                        .join("powershell")
                        .join("Microsoft.PowerShell_profile.ps1"),
                ]
            );
        }
    }

    #[test]
    fn test_pwsh_quoting() {
        assert_eq!(Pwsh.quote(""), "''");
        assert_eq!(Pwsh.quote("simple"), "'simple'");
        assert_eq!(Pwsh.quote("don't"), "'don''t'");
        assert_eq!(Pwsh.quote("say \"hello\""), "\"say `\"hello`\"\"");
        assert_eq!(Pwsh.quote("back`tick"), "\"back``tick\"");
        // assert_eq!(Pwsh.quote("price $5"), "\"price `$5\"");
    }
}
