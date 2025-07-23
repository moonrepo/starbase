use std::collections::HashMap;
use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use shell_quote::Quotable;

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

pub fn is_quoted<'a, T: Into<Quotable<'a>>>(value: T, quotes: &[&str]) -> bool {
    let value: Quotable<'_> = value.into();

    for quote in quotes {
        match value {
            Quotable::Bytes(bytes) => {
                let qb = quote.as_bytes();

                if bytes.starts_with(qb) && bytes.ends_with(qb) {
                    return true;
                }
            }
            Quotable::Text(text) => {
                if text.starts_with(quote) && text.ends_with(quote) {
                    return true;
                }
            }
        };
    }

    false
}

pub fn get_config_dir(home_dir: &Path) -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .and_then(is_absolute_dir)
        .unwrap_or_else(|| home_dir.join(".config"))
}

pub fn get_var_regex() -> regex::Regex {
    regex::Regex::new(r"\$(?<name>[A-Za-z0-9_]+)").unwrap()
}

pub fn get_var_regex_bytes() -> regex::bytes::Regex {
    regex::bytes::Regex::new(r"\$(?<name>[A-Za-z0-9_]+)").unwrap()
}

pub fn get_env_var_regex() -> regex::Regex {
    regex::Regex::new(r"\$(?<name>[A-Z0-9_]+)").unwrap()
}

pub fn get_env_key_native(key: &str) -> &str {
    let is_windows = std::env::consts::OS == "windows";

    if key == "PATH" {
        if is_windows { "Path" } else { "PATH" }
    } else if key == "HOME" {
        if is_windows { "USERPROFILE" } else { "HOME" }
    } else {
        key
    }
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

#[derive(Debug, Default)]
pub struct ProfileSet {
    pub items: HashMap<PathBuf, u8>,
}

impl ProfileSet {
    pub fn insert(mut self, path: PathBuf, order: u8) -> Self {
        self.items.insert(path, order);

        Self { items: self.items }
    }

    pub fn into_list(self) -> Vec<PathBuf> {
        let mut items = self.items.into_iter().collect::<Vec<_>>();
        items.sort_by(|a, d| a.1.cmp(&d.1));
        items.into_iter().map(|item| item.0).collect()
    }
}
