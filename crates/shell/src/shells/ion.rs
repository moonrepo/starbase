use super::Shell;
use crate::helpers::get_config_dir;
use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Ion;

impl Ion {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self
    }
}

impl Shell for Ion {
    // https://doc.redox-os.org/ion-manual/variables/05-exporting.html
    fn format_env_set(&self, key: &str, value: &str) -> String {
        format!("export {}={};", self.quote(key), self.quote(value))
    }

    fn format_env_unset(&self, key: &str) -> String {
        // TODO Not sure if correct
        format!(r#"drop {key}"#)
    }

    fn format_path_set(&self, paths: &[String]) -> String {
        // TODO Not sure if correct
        format!(r#"export PATH = "{}:{}""#, paths.join(":"), "${env::PATH}")
    }

    fn get_config_path(&self, home_dir: &Path) -> PathBuf {
        get_config_dir(home_dir).join("ion").join("initrc")
    }

    fn get_env_path(&self, home_dir: &Path) -> PathBuf {
        self.get_config_path(home_dir)
    }

    // https://doc.redox-os.org/ion-manual/general.html#xdg-app-dirs-support
    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf> {
        HashSet::<PathBuf>::from_iter([
            get_config_dir(home_dir).join("ion").join("initrc"),
            home_dir.join(".config").join("ion").join("initrc"),
        ])
        .into_iter()
        .collect()
    }

    fn quote(&self, value: &str) -> String {
        if value
            .chars()
            .all(|c| c.is_ascii_graphic() && !c.is_whitespace())
        {
            // No quoting needed for simple values
            value.to_string()
        } else {
            format!("\"{}\"", value.replace("\"", "\\\""))
        }
    }
}

impl fmt::Display for Ion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ion")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_env_var() {
        assert_eq!(
            Ion.format_env_set("PROTO_HOME", "$HOME/.proto"),
            r#"export PROTO_HOME = "$HOME/.proto""#
        );
    }

    #[test]
    fn formats_path() {
        assert_eq!(
            Ion.format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME/bin".into()]),
            r#"export PATH = "$PROTO_HOME/shims:$PROTO_HOME/bin:${env::PATH}""#
        );
    }
}
