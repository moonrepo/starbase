use starbase_styles::{Style, Stylize};
use std::path::PathBuf;
use thiserror::Error;

#[cfg(not(feature = "miette"))]
#[derive(Error, Debug)]
pub enum ArchiveError {
    #[error(
        "Unable to handle archive {}. This format requires the {} feature to be enabled.",
        .path.style(Style::Path),
        .feature.style(Style::Symbol),
    )]
    FeatureNotEnabled { feature: String, path: PathBuf },

    #[error(
        "Unable to handle archive {}, unsupported format {}.",
        .path.style(Style::Path),
        .format.style(Style::Symbol),
    )]
    UnsupportedFormat { format: String, path: PathBuf },

    #[error(
        "Unable to handle archive {}, could not determine format.",
        .path.style(Style::Path),
    )]
    UnknownFormat { path: PathBuf },
}

#[cfg(feature = "miette")]
#[derive(Error, Debug, miette::Diagnostic)]
pub enum ArchiveError {
    #[diagnostic(code(archive::feature_required))]
    #[error(
        "Unable to handle archive {}. This format requires the {} feature to be enabled.",
        .path.style(Style::Path),
        .feature.style(Style::Symbol),
    )]
    FeatureNotEnabled { feature: String, path: PathBuf },

    #[diagnostic(code(archive::unsupported_format))]
    #[error(
        "Unable to handle archive {}, unsupported format {}.",
        .path.style(Style::Path),
        .format.style(Style::Symbol),
    )]
    UnsupportedFormat { format: String, path: PathBuf },

    #[diagnostic(code(archive::unknown_format))]
    #[error(
        "Unable to handle archive {}, could not determine format.",
        .path.style(Style::Path),
    )]
    UnknownFormat { path: PathBuf },
}
