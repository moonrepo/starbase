use starbase_styles::{Style, Stylize};
use starbase_utils::fs::FsError;
use starbase_utils::glob::GlobError;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
#[cfg_attr(feature = "miette", derive(miette::Diagnostic))]
pub enum ArchiveError {
    #[error(transparent)]
    Fs(#[from] Box<FsError>),

    #[error(transparent)]
    Glob(#[from] Box<GlobError>),

    #[error(transparent)]
    Generic(#[from] Box<dyn std::error::Error>),

    #[cfg(feature = "gz")]
    #[error(transparent)]
    Gz(#[from] Box<crate::gz::GzError>),

    #[cfg(feature = "tar")]
    #[error(transparent)]
    Tar(#[from] Box<crate::tar::TarError>),

    #[cfg(feature = "zip")]
    #[error(transparent)]
    Zip(#[from] Box<crate::zip::ZipError>),

    #[cfg_attr(feature = "miette", diagnostic(code(archive::feature_required)))]
    #[error(
        "Unable to handle archive {}. This format requires the {} feature to be enabled.",
        .path.style(Style::Path),
        .feature.style(Style::Symbol),
    )]
    FeatureNotEnabled { feature: String, path: PathBuf },

    #[cfg_attr(feature = "miette", diagnostic(code(archive::unsupported_format)))]
    #[error(
        "Unable to handle archive {}, unsupported format {}.",
        .path.style(Style::Path),
        .format.style(Style::Symbol),
    )]
    UnsupportedFormat { format: String, path: PathBuf },

    #[cfg_attr(feature = "miette", diagnostic(code(archive::unknown_format)))]
    #[error(
        "Unable to handle archive {}, could not determine format.",
        .path.style(Style::Path),
    )]
    UnknownFormat { path: PathBuf },
}

impl From<FsError> for ArchiveError {
    fn from(e: FsError) -> ArchiveError {
        ArchiveError::Fs(Box::new(e))
    }
}

impl From<GlobError> for ArchiveError {
    fn from(e: GlobError) -> ArchiveError {
        ArchiveError::Glob(Box::new(e))
    }
}

#[cfg(feature = "gz")]
impl From<crate::gz::GzError> for ArchiveError {
    fn from(e: crate::gz::GzError) -> ArchiveError {
        ArchiveError::Gz(Box::new(e))
    }
}

#[cfg(feature = "tar")]
impl From<crate::tar::TarError> for ArchiveError {
    fn from(e: crate::tar::TarError) -> ArchiveError {
        ArchiveError::Tar(Box::new(e))
    }
}

#[cfg(feature = "zip")]
impl From<crate::zip::ZipError> for ArchiveError {
    fn from(e: crate::zip::ZipError) -> ArchiveError {
        ArchiveError::Zip(Box::new(e))
    }
}
