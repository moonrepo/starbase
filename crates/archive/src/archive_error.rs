use starbase_styles::{Style, Stylize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
#[cfg_attr(feature = "miette", derive(miette::Diagnostic))]
pub enum ArchiveError {
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
