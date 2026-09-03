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

    #[error("Failed to walk directory {}, as the walk was aborted before completing.\n{error}", .dir.style(Style::Path))]
    WalkAborted {
        dir: PathBuf,
        #[source]
        error: Box<jwalk::Error>,
    },
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

    #[diagnostic(code(glob::walk_aborted))]
    #[error("Failed to walk directory {}, as the walk was aborted before completing.", .dir.style(Style::Path))]
    WalkAborted {
        dir: PathBuf,
        #[source]
        error: Box<jwalk::Error>,
    },
}

impl From<FsError> for GlobError {
    fn from(e: FsError) -> GlobError {
        GlobError::Fs(Box::new(e))
    }
}
