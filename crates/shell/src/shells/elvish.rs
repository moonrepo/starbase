use super::Shell;
use crate::helpers::{get_config_dir, get_env_var_regex};
use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Elvish;

impl Elvish {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self
    }
}

fn format(value: impl AsRef<str>) -> String {
    get_env_var_regex()
        .replace_all(value.as_ref(), "$$E:$name")
        .replace("$E:HOME", "{~}")
}

// https://elv.sh/ref/command.html#using-elvish-interactivelyn
impl Shell for Elvish {
    fn format_env_export(&self, key: &str, value: &str) -> String {
        format!("set-env {key} {}", format(value))
    }

    fn format_path_export(&self, paths: &[String]) -> String {
        format!("set paths [{} $@paths]", format(paths.join(" ")))
    }

    fn get_config_path(&self, home_dir: &Path) -> PathBuf {
        get_config_dir(home_dir).join("elvish").join("rc.elv")
    }

    fn get_env_path(&self, home_dir: &Path) -> PathBuf {
        self.get_config_path(home_dir)
    }

    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf> {
        #[allow(unused_mut)]
        let mut profiles = HashSet::<PathBuf>::from_iter([
            get_config_dir(home_dir).join("elvish").join("rc.elv"),
            home_dir.join(".config").join("elvish").join("rc.elv"),
            home_dir.join(".elvish").join("rc.elv"), // Legacy
        ]);

        #[cfg(windows)]
        {
            profiles.insert(
                home_dir
                    .join("AppData")
                    .join("Roaming")
                    .join("elvish")
                    .join("rc.elv"),
            );
        }

        profiles.into_iter().collect()
    }
}

impl fmt::Display for Elvish {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "elvish")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_env_var() {
        assert_eq!(
            Elvish.format_env_export("PROTO_HOME", "$HOME/.proto"),
            r#"set-env PROTO_HOME {~}/.proto"#
        );
        assert_eq!(Elvish.format_env_export("FOO", "bar"), r#"set-env FOO bar"#);
    }

    #[test]
    fn formats_path() {
        assert_eq!(
            Elvish.format_path_export(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"set paths [$E:PROTO_HOME/shims $E:PROTO_HOME/bin $@paths]"#
        );
    }
}
