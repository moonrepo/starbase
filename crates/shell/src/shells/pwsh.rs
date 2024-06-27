use super::{Shell, ShellCommand};
use crate::helpers::get_env_var_regex;
use std::collections::HashSet;
use std::env::consts;
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
            format!("$env:{key} = {};", join_path(value))
        } else {
            format!(r#"$env:{key} = "{}";"#, format(value))
        }
    }

    fn format_env_unset(&self, key: &str) -> String {
        format!(r#"Remove-Item -LiteralPath "env:{key}";"#)
    }

    fn format_path_set(&self, paths: &[String]) -> String {
        let newline = if consts::OS == "windows" {
            "\r\n"
        } else {
            "\n"
        };

        let mut value = format!("$env:PATH = @({newline}");

        for path in paths {
            value.push_str(&format!("  ({}),{newline}", join_path(path)))
        }

        value.push_str("  $env:PATH");
        value.push_str(newline);
        value.push_str(") -join [IO.PATH]::PathSeparator;");
        value
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
}

impl fmt::Display for Pwsh {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "pwsh")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
