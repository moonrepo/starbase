use crate::fs::FsError;
use starbase_styles::{Style, Stylize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
#[cfg_attr(feature = "miette", derive(miette::Diagnostic))]
pub enum TomlError {
    #[cfg_attr(feature = "miette", diagnostic(transparent))]
    #[error(transparent)]
    Fs(#[from] FsError),

    #[cfg_attr(feature = "miette", diagnostic(code(toml::format)))]
    #[error("Failed to format TOML.")]
    Format {
        #[source]
        error: toml::ser::Error,
    },

    #[cfg_attr(feature = "miette", diagnostic(code(toml::parse)))]
    #[error("Failed to parse TOML.")]
    Parse {
        #[source]
        error: toml::de::Error,
    },

    #[cfg_attr(feature = "miette", diagnostic(code(toml::parse_file)))]
    #[error("Failed to parse TOML file {}.", .path.style(Style::Path))]
    ReadFile {
        path: PathBuf,
        #[source]
        error: toml::de::Error,
    },

    #[cfg_attr(feature = "miette", diagnostic(code(toml::format_file)))]
    #[error("Failed to format TOML for file {}.", .path.style(Style::Path))]
    WriteFile {
        path: PathBuf,
        #[source]
        error: toml::ser::Error,
    },
}
