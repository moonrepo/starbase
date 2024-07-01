use super::{Shell, ShellCommand};
use crate::helpers::{get_env_var_regex, normalize_newlines};
use crate::hooks::Hook;
use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Pwsh;

impl Pwsh {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self
    }
}

fn format(value: impl AsRef<str>) -> String {
    get_env_var_regex()
        .replace_all(value.as_ref(), "$$env:$name")
        .replace("$env:HOME", "$HOME")
}

fn join_path(value: impl AsRef<str>) -> String {
    let parts = value
        .as_ref()
        .split('/')
        .map(|part| {
            if part.starts_with('$') {
                part.to_owned()
            } else {
                format!("\"{}\"", part)
            }
        })
        .collect::<Vec<_>>();

    format(format!("Join-Path {}", parts.join(" ")))
}

// https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_profiles?view=powershell-7.4
impl Shell for Pwsh {
    fn format_env_set(&self, key: &str, value: &str) -> String {
        if value.contains('/') {
            format!("$env:{} = {};", key, join_path(value))
        } else {
            format!("$env:{} = {};", key, self.quote(format(value).as_str()))
        }
    }

    fn format_env_unset(&self, key: &str) -> String {
        format!(r#"Remove-Item -LiteralPath "env:{key}";"#)
    }

    fn format_path_set(&self, paths: &[String]) -> String {
        let mut value = "$env:PATH = @(\n".to_string();

        for path in paths {
            value.push_str(&format!("  ({}),\n", join_path(path)))
        }

        value.push_str("  $env:PATH\n");
        value.push_str(") -join [IO.PATH]::PathSeparator;");

        normalize_newlines(value)
    }

    fn format_hook(&self, hook: Hook) -> Result<String, crate::ShellError> {
        Ok(hook.render_template(self, r#"
using namespace System;
using namespace System.Management.Automation;

$hook = [EventHandler[LocationChangedEventArgs]] {
  param([object] $source, [LocationChangedEventArgs] $eventArgs)
  end {
{export_env}
{export_path}
  }
};

$currentAction = $ExecutionContext.SessionState.InvokeCommand.LocationChangedAction;

if ($currentAction) {
  $ExecutionContext.SessionState.InvokeCommand.LocationChangedAction = [Delegate]::Combine($currentAction, $hook);
} else {
  $ExecutionContext.SessionState.InvokeCommand.LocationChangedAction = $hook;
};
"#, "    "))
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
        let mut profiles = HashSet::new();

        #[cfg(windows)]
        {
            let docs_dir = home_dir.join("Documents");

            profiles.extend([
                docs_dir
                    .join("PowerShell")
                    .join("Microsoft.PowerShell_profile.ps1"),
                docs_dir.join("PowerShell").join("Profile.ps1"),
            ]);
        }

        #[cfg(unix)]
        {
            use crate::helpers::get_config_dir;

            profiles.extend([
                get_config_dir(home_dir)
                    .join("powershell")
                    .join("Microsoft.PowerShell_profile.ps1"),
                home_dir
                    .join(".config")
                    .join("powershell")
                    .join("Microsoft.PowerShell_profile.ps1"),
                get_config_dir(home_dir)
                    .join("powershell")
                    .join("profile.ps1"),
                home_dir
                    .join(".config")
                    .join("powershell")
                    .join("profile.ps1"),
            ]);
        }

        profiles.into_iter().collect()
    }

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
                let escaped: String = value
                    .replace('`', "``")
                    .replace('$', "`$")
                    .replace('"', "`\"");
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
    }

    #[test]
    fn formats_path() {
        assert_eq!(
            Pwsh.format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()])
                .replace("\r\n", "\n"),
            r#"$env:PATH = @(
  (Join-Path $env:PROTO_HOME "shims"),
  (Join-Path $env:PROTO_HOME "bin"),
  $env:PATH
) -join [IO.PATH]::PathSeparator;"#
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

        assert_snapshot!(Pwsh.format_hook(hook).unwrap());
    }

    #[test]
    fn test_pwsh_quoting() {
        assert_eq!(Pwsh.quote(""), "''");
        assert_eq!(Pwsh.quote("simple"), "'simple'");
        assert_eq!(Pwsh.quote("don't"), "'don''t'");
        assert_eq!(Pwsh.quote("say \"hello\""), "\"say `\"hello`\"\"");
        assert_eq!(Pwsh.quote("back`tick"), "\"back``tick\"");
        assert_eq!(Pwsh.quote("price $5"), "\"price `$5\"");
    }
}
