use super::Shell;
use crate::helpers::{get_config_dir, get_env_var_regex, NEWLINE};
use crate::hooks::OnCdHook;
use std::collections::HashSet;
use std::env::consts;
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
    fn format_env_set(&self, key: &str, value: &str) -> String {
        format!(r#"$env.{key} = '{value}'"#)
    }

    fn format_env_unset(&self, key: &str) -> String {
        format!(r#"hide-env {key}"#)
    }

    // https://www.nushell.sh/book/configuration.html#path-configuration
    fn format_path_set(&self, paths: &[String]) -> String {
        let path_name = if consts::OS == "windows" {
            "Path"
        } else {
            "PATH"
        };

        let env_regex = get_env_var_regex();
        let path_var = format!("$env.{path_name}");
        let mut value = format!("{path_var} = {path_var} | split row (char esep){}", NEWLINE);

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

            value.push_str(NEWLINE);
        }

        value.push_str("  | uniq");
        value
    }

    fn format_on_cd_hook(&self, hook: OnCdHook) -> Result<String, crate::ShellError> {
        Ok(hook.render_template(
            self,
            r#"
# {prefix} hook
$env.config = ( $env.config | upsert hooks.env_change.PWD { |config|
    let list = ($config | get -i hooks.env_change.PWD) | default []

    $list | append { |before, after|
{export_env}
{export_path}
    }
})"#,
            "        ",
        ))
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

impl fmt::Display for Nu {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "nu")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use starbase_sandbox::assert_snapshot;

    #[test]
    fn formats_env_var() {
        assert_eq!(
            Nu.format_env_set("PROTO_HOME", "$HOME/.proto"),
            r#"$env.PROTO_HOME = '$HOME/.proto'"#
        );
    }

    #[cfg(unix)]
    #[test]
    fn formats_path() {
        assert_eq!(
            Nu.format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"$env.PATH = $env.PATH | split row (char esep)
  | prepend ($env.PROTO_HOME | path join shims)
  | prepend ($env.PROTO_HOME | path join bin)
  | uniq"#
        );

        assert_eq!(
            Nu.format_path_set(&["$HOME/with/sub/dir".into(), "/some/abs/path/bin".into()]),
            r#"$env.PATH = $env.PATH | split row (char esep)
  | prepend ($env.HOME | path join with sub dir)
  | prepend /some/abs/path/bin
  | uniq"#
        );
    }

    #[cfg(windows)]
    #[test]
    fn formats_path() {
        assert_eq!(
            Nu.format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"$env.Path = $env.Path | split row (char esep)
  | prepend ($env.PROTO_HOME | path join shims)
  | prepend ($env.PROTO_HOME | path join bin)
  | uniq"#
        );

        assert_eq!(
            Nu.format_path_set(&["$HOME/with/sub/dir".into(), "/some/abs/path/bin".into()]),
            r#"$env.Path = $env.Path | split row (char esep)
  | prepend ($env.HOME | path join with sub dir)
  | prepend /some/abs/path/bin
  | uniq"#
        );
    }

    #[cfg(unix)]
    #[test]
    fn formats_cd_hook() {
        let mut hook = OnCdHook {
            prefix: "starbase".into(),
            ..OnCdHook::default()
        };

        hook.paths
            .extend(["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]);
        hook.env.extend([
            ("PROTO_HOME".into(), Some("$HOME/.proto".into())),
            ("PROTO_ROOT".into(), None),
        ]);

        assert_snapshot!(Nu.format_on_cd_hook(hook).unwrap());
    }
}
