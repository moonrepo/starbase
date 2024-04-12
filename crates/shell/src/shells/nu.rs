use super::Shell;
use crate::helpers::{get_config_dir, get_env_var_regex};
use std::collections::HashSet;
use std::env::consts;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Nu;

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
    fn format_env_export(&self, key: &str, value: &str) -> String {
        format!(r#"$env.{key} = '{value}'"#)
    }

    // https://www.nushell.sh/book/configuration.html#path-configuration
    fn format_path_export(&self, paths: &[String]) -> String {
        let (path_name, newline) = if consts::OS == "windows" {
            ("Path", "\r\n")
        } else {
            ("PATH", "\n")
        };

        let env_regex = get_env_var_regex();
        let path_var = format!("$env.{path_name}");
        let mut value = format!("{path_var} = {path_var} | split row (char esep){newline}");

        for path in paths {
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

            value.push_str(newline);
        }

        value.push_str("  | uniq");
        value
    }

    fn get_main_profile_path(&self, home_dir: &Path) -> PathBuf {
        get_config_dir(home_dir).join("nushell").join("config.nu")
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
}
