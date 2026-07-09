use starbase_styles::{Style, Stylize};
use starbase_utils::fs::FsError;
use std::path::PathBuf;
use thiserror::Error;

#[cfg(not(feature = "miette"))]
#[derive(Error, Debug)]
pub enum PkgError {
    #[error(transparent)]
    Fs(#[from] Box<FsError>),

    #[error("Unable to find a payload in macOS package {}.", .path.style(Style::Path))]
    MissingPayload { path: PathBuf },

    #[error("Failed to unpack archive.\n{error}")]
    UnpackFailure {
        #[source]
        error: Box<std::io::Error>,
    },
}

#[cfg(feature = "miette")]
#[derive(Error, Debug, miette::Diagnostic)]
pub enum PkgError {
    #[error(transparent)]
    Fs(#[from] Box<FsError>),

    #[diagnostic(code(pkg::unpack::missing_payload))]
    #[error("Unable to find a payload in macOS package {}.", .path.style(Style::Path))]
    MissingPayload { path: PathBuf },

    #[diagnostic(code(pkg::unpack))]
    #[error("Failed to unpack archive.")]
    UnpackFailure {
        #[source]
        error: Box<std::io::Error>,
    },
}

impl From<FsError> for PkgError {
    fn from(e: FsError) -> PkgError {
        PkgError::Fs(Box::new(e))
    }
}
