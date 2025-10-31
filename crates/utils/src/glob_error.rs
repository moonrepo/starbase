use crate::fs_error::FsError;
use starbase_styles::{Style, Stylize};
use std::path::PathBuf;
use thiserror::Error;
use wax::BuildError;

/// Glob errors.
#[cfg(not(feature = "miette"))]
#[derive(Error, Debug)]
pub enum GlobError {
    #[error(transparent)]
    Fs(#[from] Box<FsError>),

    #[error("Failed to create glob from pattern {}.\n{error}", .glob.style(Style::File))]
    Create {
        glob: String,
        #[source]
        error: Box<BuildError>,
    },

    #[error("Failed to normalize glob path {}.", .path.style(Style::Path))]
    InvalidPath { path: PathBuf },
}

/// Glob errors.
#[cfg(feature = "miette")]
#[derive(Error, Debug, miette::Diagnostic)]
pub enum GlobError {
    #[error(transparent)]
    Fs(#[from] Box<FsError>),

    #[diagnostic(code(glob::create))]
    #[error("Failed to create glob from pattern {}.", .glob.style(Style::File))]
    Create {
        glob: String,
        #[source]
        error: Box<BuildError>,
    },

    #[diagnostic(code(glob::invalid_path))]
    #[error("Failed to normalize glob path {}.", .path.style(Style::Path))]
    InvalidPath { path: PathBuf },
}

impl From<FsError> for GlobError {
    fn from(e: FsError) -> GlobError {
        GlobError::Fs(Box::new(e))
    }
}
