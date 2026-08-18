use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// A single command line argument, in both its shell-quoted and raw forms.
#[derive(Debug)]
pub struct Arg {
    /// The value as it should appear within a shell, e.g. `"value"`.
    /// Falls back to `value` when quoting isn't required.
    pub quoted_value: Option<OsString>,

    /// The raw, unquoted value, e.g. `value`.
    pub value: OsString,
}

impl Arg {
    /// Return the quoted value if present, otherwise the raw value.
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
