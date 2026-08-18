use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct Arg {
    // In shells: "value"
    pub quoted_value: Option<OsString>,

    // Not in shells: value
    pub value: OsString,
}

impl Arg {
    pub fn as_os_str(&self) -> &OsStr {
        self.quoted_value.as_ref().unwrap_or(&self.value)
    }
}

impl AsRef<OsStr> for Arg {
    fn as_ref(&self) -> &OsStr {
        self.as_os_str()
    }
}

impl From<&str> for Arg {
    fn from(value: &str) -> Self {
        Self::from(OsString::from(value))
    }
}

impl From<&String> for Arg {
    fn from(value: &String) -> Self {
        Self::from(OsString::from(value))
    }
}

impl From<String> for Arg {
    fn from(value: String) -> Self {
        Self::from(OsString::from(value))
    }
}

impl From<&OsStr> for Arg {
    fn from(value: &OsStr) -> Self {
        Self::from(value.to_os_string())
    }
}

impl From<&OsString> for Arg {
    fn from(value: &OsString) -> Self {
        Self::from(value.to_os_string())
    }
}

impl From<OsString> for Arg {
    fn from(value: OsString) -> Self {
        Self {
            quoted_value: None,
            value,
        }
    }
}

impl From<&Path> for Arg {
    fn from(value: &Path) -> Self {
        Self::from(value.as_os_str())
    }
}

impl From<&PathBuf> for Arg {
    fn from(value: &PathBuf) -> Self {
        Self::from(value.as_os_str())
    }
}

impl From<PathBuf> for Arg {
    fn from(value: PathBuf) -> Self {
        Self::from(value.into_os_string())
    }
}
