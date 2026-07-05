use starbase_styles::{Style, Stylize};
use starbase_utils::fs::FsError;
use std::path::PathBuf;
use thiserror::Error;

#[cfg(not(feature = "miette"))]
#[derive(Error, Debug)]
pub enum FileError {
    #[error(transparent)]
    Fs(#[from] Box<FsError>),

    #[error("Failed to add source {} to archive.\n{error}", .source.style(Style::Path))]
    AddFailure {
        source: PathBuf,
        #[source]
        error: Box<std::io::Error>,
    },

    #[error("Directories cannot be packed into a single-file archive. Use {} instead.", "tar".style(Style::Symbol))]
    NoDirs,

    #[error("Only 1 file can be packed into a single-file archive, received more than 1.")]
    OneFile,

    #[error("Failed to pack archive.\n{error}")]
    PackFailure {
        #[source]
        error: Box<std::io::Error>,
    },

    #[error("Failed to unpack archive.\n{error}")]
    UnpackFailure {
        #[source]
        error: Box<std::io::Error>,
    },
}

#[cfg(feature = "miette")]
#[derive(Error, Debug, miette::Diagnostic)]
pub enum FileError {
    #[error(transparent)]
    Fs(#[from] Box<FsError>),

    #[diagnostic(code(file::pack::add))]
    #[error("Failed to add source {} to archive.", .source.style(Style::Path))]
    AddFailure {
        source: PathBuf,
        #[source]
        error: Box<std::io::Error>,
    },

    #[diagnostic(code(file::pack::no_dirs))]
    #[error("Directories cannot be packed into a single-file archive. Use {} instead.", "tar".style(Style::Symbol))]
    NoDirs,

    #[diagnostic(code(file::pack::one_file))]
    #[error("Only 1 file can be packed into a single-file archive, received more than 1.")]
    OneFile,

    #[diagnostic(code(file::pack))]
    #[error("Failed to pack archive.")]
    PackFailure {
        #[source]
        error: Box<std::io::Error>,
    },

    #[diagnostic(code(file::unpack))]
    #[error("Failed to unpack archive.")]
    UnpackFailure {
        #[source]
        error: Box<std::io::Error>,
    },
}

impl From<FsError> for FileError {
    fn from(e: FsError) -> FileError {
        FileError::Fs(Box::new(e))
    }
}
