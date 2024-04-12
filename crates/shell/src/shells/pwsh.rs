use super::Shell;
use crate::helpers::{get_config_dir, get_env_var_regex};
use std::collections::HashSet;
use std::env::consts;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Pwsh;

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
    fn format_env_export(&self, key: &str, value: &str) -> String {
        if value.contains('/') {
            format!("$env:{key} = {}", join_path(value))
        } else {
            format!(r#"$env:{key} = "{}""#, format(value))
        }
    }

    fn format_path_export(&self, paths: &[String]) -> String {
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
        value.push_str(") -join [IO.PATH]::PathSeparator");
        value
    }

    fn get_main_profile_path(&self, home_dir: &Path) -> PathBuf {
        #[cfg(windows)]
        {
            home_dir
                .join("Documents")
                .join("PowerShell")
                .join("Microsoft.PowerShell_profile.ps1")
        }

        #[cfg(not(windows))]
        {
            get_config_dir(home_dir)
                .join("powershell")
                .join("Microsoft.PowerShell_profile.ps1")
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

        #[cfg(not(windows))]
        {
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
