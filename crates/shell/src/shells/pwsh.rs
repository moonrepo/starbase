use super::powershell::PowerShell;
use super::{Shell, ShellCommand};
use crate::helpers::{ProfileSet, normalize_newlines};
use crate::hooks::*;
use std::env;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Pwsh {
    inner: PowerShell,
}

impl Pwsh {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            inner: PowerShell::new(),
        }
    }
}

// https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_profiles?view=powershell-7.4
impl Shell for Pwsh {
    fn format(&self, statement: Statement<'_>) -> String {
        self.inner.format(statement)
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
        self.inner.get_exec_command()
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
        self.inner.quote(value)
    }

    // https://learn.microsoft.com/en-us/powershell/scripting/learn/deep-dives/everything-about-string-substitutions?view=powershell-7.5
    fn requires_expansion(&self, value: &str) -> bool {
        self.inner.requires_expansion(value)
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
            Pwsh::new().format_env_set("PROTO_HOME", "$HOME/.proto"),
            r#"$env:PROTO_HOME = Join-Path $HOME ".proto";"#
        );
        assert_eq!(
            Pwsh::new().format_env_set("PROTO_HOME", "$HOME"),
            r#"$env:PROTO_HOME = "$HOME";"#
        );
        assert_eq!(
            Pwsh::new().format_env_set("BOOL", "true"),
            r#"$env:BOOL = 'true';"#
        );
        assert_eq!(
            Pwsh::new().format_env_set("STRING", "a b c"),
            r#"$env:STRING = 'a b c';"#
        );
    }

    #[test]
    fn formats_cd_hook() {
        let hook = Hook::OnChangeDir {
            command: "starbase hook pwsh".into(),
            function: "_starbase_hook".into(),
        };

        assert_snapshot!(Pwsh::new().format_hook(hook).unwrap());
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
        assert_eq!(Pwsh::new().quote(""), "''");
        assert_eq!(Pwsh::new().quote("simple"), "'simple'");
        assert_eq!(Pwsh::new().quote("don't"), "'don''t'");
        assert_eq!(Pwsh::new().quote("say \"hello\""), "\"say `\"hello`\"\"");
        assert_eq!(Pwsh::new().quote("back`tick"), "\"back``tick\"");
        // assert_eq!(Pwsh::new().quote("price $5"), "\"price `$5\"");
    }
}
