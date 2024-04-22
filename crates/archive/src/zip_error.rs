use starbase_styles::{Style, Stylize};
use std::path::PathBuf;
use thiserror::Error;
use zip::result::ZipError as BaseZipError;

#[cfg(not(feature = "miette"))]
#[derive(Error, Debug)]
pub enum ZipError {
    #[error("Failed to add source {} to archive.\n{error}", .source.style(Style::Path))]
    AddFailure {
        source: PathBuf,
        #[source]
        error: Box<BaseZipError>,
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
        error: Box<BaseZipError>,
    },

    #[error("Failed to unpack archive.\n{error}")]
    UnpackFailure {
        #[source]
        error: Box<BaseZipError>,
    },
}

#[cfg(feature = "miette")]
#[derive(Error, Debug, miette::Diagnostic)]
pub enum ZipError {
    #[diagnostic(code(zip::pack::add))]
    #[error("Failed to add source {} to archive.", .source.style(Style::Path))]
    AddFailure {
        source: PathBuf,
        #[source]
        error: Box<BaseZipError>,
    },

    #[diagnostic(code(zip::unpack::extract))]
    #[error("Failed to extract {} from archive.", .source.style(Style::Path))]
    ExtractFailure {
        source: PathBuf,
        #[source]
        error: Box<std::io::Error>,
    },

    #[diagnostic(code(zip::pack::finish))]
    #[error("Failed to pack archive.")]
    PackFailure {
        #[source]
        error: Box<BaseZipError>,
    },

    #[diagnostic(code(zip::unpack::finish))]
    #[error("Failed to unpack archive.")]
    UnpackFailure {
        #[source]
        error: Box<BaseZipError>,
    },
}
