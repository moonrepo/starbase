use crate::arg::Arg;
use std::ffi::{OsStr, OsString};

#[derive(Debug)]
pub enum Executable {
    /// Single file name: git
    Binary(Arg),

    /// Full script: git commit --allow-empty
    Script(OsString),
}

impl Executable {
    pub fn as_os_str(&self) -> &OsStr {
        match self {
            Self::Binary(inner) => &inner.value,
            Self::Script(inner) => inner,
        }
    }

    pub fn requires_shell(&self) -> bool {
        match self {
            Self::Binary(_) => false,
            Self::Script(_) => true,
        }
    }
}
