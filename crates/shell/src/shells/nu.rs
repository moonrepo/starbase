use super::Shell;
use crate::helpers::{get_config_dir, get_env_var_regex};
use std::collections::HashSet;
use std::env::consts;
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_env_var() {
        assert_eq!(
            Nu.format_env_export("PROTO_HOME", "$HOME/.proto"),
            r#"$env.PROTO_HOME = '$HOME/.proto'"#
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn formats_path() {
        assert_eq!(
            Nu.format_path_export(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"$env.PATH = $env.PATH | split row (char esep)
  | prepend ($env.PROTO_HOME | path join shims)
  | prepend ($env.PROTO_HOME | path join bin)
  | uniq"#
        );

        assert_eq!(
            Nu.format_path_export(&["$HOME/with/sub/dir".into(), "/some/abs/path/bin".into()]),
            r#"$env.PATH = $env.PATH | split row (char esep)
  | prepend ($env.HOME | path join with sub dir)
  | prepend /some/abs/path/bin
  | uniq"#
        );
    }
}
