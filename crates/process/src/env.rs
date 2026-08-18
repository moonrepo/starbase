use std::ffi::{OsStr, OsString};

#[derive(Debug, PartialEq)]
pub enum Env {
    /// Always set and overwrite system var
    Set(OsString),

    /// Only set if system var is not set
    SetIfMissing(OsString),

    /// Unset system var and don't inherit
    Unset,
}

impl Env {
    pub fn as_os_str(&self) -> Option<&OsStr> {
        self.get_value().map(|value| value.as_os_str())
    }

    pub fn get_value(&self) -> Option<&OsString> {
        match self {
            Env::Set(value) => Some(value),
            Env::SetIfMissing(value) => Some(value),
            Env::Unset => None,
        }
    }
}
