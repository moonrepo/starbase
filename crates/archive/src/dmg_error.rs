use starbase_styles::{Style, Stylize};
use starbase_utils::fs::FsError;
use std::path::PathBuf;
use thiserror::Error;

#[cfg(not(feature = "miette"))]
#[derive(Error, Debug)]
pub enum DmgError {
    #[error(transparent)]
    Fs(#[from] Box<FsError>),

    #[error("Unable to find a mounted volume for macOS disk image {}.", .path.style(Style::Path))]
    MissingVolume { path: PathBuf },

    #[error("Failed to unpack archive.\n{error}")]
    UnpackFailure {
        #[source]
        error: Box<std::io::Error>,
    },
}

#[cfg(feature = "miette")]
#[derive(Error, Debug, miette::Diagnostic)]
pub enum DmgError {
    #[error(transparent)]
    Fs(#[from] Box<FsError>),

    #[diagnostic(code(dmg::unpack::missing_volume))]
    #[error("Unable to find a mounted volume for macOS disk image {}.", .path.style(Style::Path))]
    MissingVolume { path: PathBuf },

    #[diagnostic(code(dmg::unpack))]
    #[error("Failed to unpack archive.")]
    UnpackFailure {
        #[source]
        error: Box<std::io::Error>,
    },
}

impl From<FsError> for DmgError {
    fn from(e: FsError) -> DmgError {
        DmgError::Fs(Box::new(e))
    }
}
