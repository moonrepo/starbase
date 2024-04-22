use starbase_styles::{Style, Stylize};
use std::path::PathBuf;
use thiserror::Error;

#[cfg(not(feature = "miette"))]
#[derive(Error, Debug)]
pub enum TarError {
    #[error("Failed to add source {} to archive.\n{error}", .source.style(Style::Path))]
    AddFailure {
        source: PathBuf,
        #[source]
        error: Box<std::io::Error>,
    },

    #[error("Failed to extract {} from archive.\n{error}", .source.style(Style::Path))]
    ExtractFailure {
        source: PathBuf,
        #[source]
        error: Box<std::io::Error>,
    },

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
pub enum TarError {
    #[diagnostic(code(tar::pack::add))]
    #[error("Failed to add source {} to archive.", .source.style(Style::Path))]
    AddFailure {
        source: PathBuf,
        #[source]
        error: Box<std::io::Error>,
    },

    #[diagnostic(code(tar::unpack::extract))]
    #[error("Failed to extract {} from archive.", .source.style(Style::Path))]
    ExtractFailure {
        source: PathBuf,
        #[source]
        error: Box<std::io::Error>,
    },

    #[diagnostic(code(tar::pack))]
    #[error("Failed to pack archive.")]
    PackFailure {
        #[source]
        error: Box<std::io::Error>,
    },

    #[diagnostic(code(tar::unpack))]
    #[error("Failed to unpack archive.")]
    UnpackFailure {
        #[source]
        error: Box<std::io::Error>,
    },
}
