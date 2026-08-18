use crate::arg::Arg;
use std::ffi::{OsStr, OsString};

/// What a [`Command`](crate::Command) runs: a single binary, or a full
/// shell script.
#[derive(Debug)]
pub enum Executable {
    /// Single file name: git
    Binary(Arg),

    /// Full script: git commit --allow-empty
    Script(OsString),
}

impl Executable {
    /// Return the raw binary name or script contents.
    pub fn as_os_str(&self) -> &OsStr {
        match self {
            Self::Binary(inner) => &inner.value,
            Self::Script(inner) => inner,
        }
    }

    /// Return true if this executable must be run inside a shell.
    pub fn requires_shell(&self) -> bool {
        match self {
            Self::Binary(_) => false,
            Self::Script(_) => true,
        }
    }
}
