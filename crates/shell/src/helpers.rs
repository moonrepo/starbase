use std::collections::HashMap;
use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[cfg(unix)]
pub static PATH_DELIMITER: &str = ":";

#[cfg(windows)]
pub static PATH_DELIMITER: &str = ";";

pub fn is_absolute_dir(value: OsString) -> Option<PathBuf> {
    let dir = PathBuf::from(&value);

    if !value.is_empty() && dir.is_absolute() {
        Some(dir)
    } else {
        None
    }
}

pub fn get_config_dir(home_dir: &Path) -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .and_then(is_absolute_dir)
        .unwrap_or_else(|| home_dir.join(".config"))
}

pub fn get_env_var_regex() -> regex::Regex {
    regex::Regex::new(r"\$(?<name>[A-Z0-9_]+)").unwrap()
}

pub fn normalize_newlines(content: impl AsRef<str>) -> String {
    let content = content.as_ref().trim();

    #[cfg(windows)]
    {
        content.replace('\r', "").replace('\n', "\r\n")
    }

    #[cfg(unix)]
    {
        content.replace('\r', "")
    }
}

#[derive(Default)]
pub struct ProfileSet {
    items: HashMap<PathBuf, u8>,
}

impl ProfileSet {
    pub fn insert(mut self, path: PathBuf, order: u8) -> Self {
        self.items.insert(path, order);

        Self { items: self.items }
    }

    pub fn to_list(self) -> Vec<PathBuf> {
        let mut items = self.items.into_iter().collect::<Vec<_>>();
        items.sort_by(|a, d| d.1.cmp(&a.1));
        items.into_iter().map(|item| item.0).collect()
    }
}
