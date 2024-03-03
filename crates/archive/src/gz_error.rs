use starbase_styles::{Style, Stylize};
use std::path::PathBuf;
use thiserror::Error;

#[cfg(not(feature = "miette"))]
#[derive(Error, Debug)]
pub enum GzError {
    #[error("Failed to add source {} to archive.\n{error}", .source.style(Style::Path))]
    AddFailure {
        source: PathBuf,
        #[source]
        error: std::io::Error,
    },

    #[error("Failed to extract {} from archive.\n{error}", .source.style(Style::Path))]
    ExtractFailure {
        source: PathBuf,
        #[source]
        error: std::io::Error,
    },

    #[error("Failed to pack archive.\n{error}")]
    PackFailure {
        #[source]
        error: std::io::Error,
    },

    #[error("Failed to unpack archive.\n{error}")]
    UnpackFailure {
        #[source]
        error: std::io::Error,
    },
}

#[cfg(feature = "miette")]
#[derive(Error, Debug, miette::Diagnostic)]
pub enum GzError {
    #[diagnostic(code(gz::pack::add))]
    #[error("Failed to add source {} to archive.", .source.style(Style::Path))]
    AddFailure {
        source: PathBuf,
        #[source]
        error: std::io::Error,
    },

    #[diagnostic(code(gz::unpack::extract))]
    #[error("Failed to extract {} from archive.", .source.style(Style::Path))]
    ExtractFailure {
        source: PathBuf,
        #[source]
        error: std::io::Error,
    },

    #[diagnostic(code(gz::pack::finish))]
    #[error("Failed to pack archive.")]
    PackFailure {
        #[source]
        error: std::io::Error,
    },

    #[diagnostic(code(gz::unpack::finish))]
    #[error("Failed to unpack archive.")]
    UnpackFailure {
        #[source]
        error: std::io::Error,
    },
}
