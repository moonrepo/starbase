use crate::fs::FsError;
use starbase_styles::{Style, Stylize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
#[cfg_attr(feature = "miette", derive(miette::Diagnostic))]
pub enum JsonError {
    #[cfg_attr(feature = "miette", diagnostic(transparent))]
    #[error(transparent)]
    Fs(#[from] FsError),

    #[cfg_attr(feature = "miette", diagnostic(code(json::clean)))]
    #[error("Failed to clean comments and trailing commas from JSON.")]
    Clean {
        #[source]
        error: std::io::Error,
    },

    #[cfg_attr(feature = "miette", diagnostic(code(json::clean_file)))]
    #[error("Failed to clean comments and trailing commas in JSON file {}.", .path.style(Style::Path))]
    CleanFile {
        path: PathBuf,
        #[source]
        error: std::io::Error,
    },

    #[cfg_attr(feature = "miette", diagnostic(code(json::format)))]
    #[error("Failed to format JSON.")]
    Format {
        #[source]
        error: serde_json::Error,
    },

    #[cfg_attr(feature = "miette", diagnostic(code(json::parse)))]
    #[error("Failed to parse JSON.")]
    Parse {
        #[source]
        error: serde_json::Error,
    },

    #[cfg_attr(feature = "miette", diagnostic(code(json::parse_file)))]
    #[error("Failed to parse JSON file {}.", .path.style(Style::Path))]
    ReadFile {
        path: PathBuf,
        #[source]
        error: serde_json::Error,
    },

    #[cfg_attr(feature = "miette", diagnostic(code(json::format_file)))]
    #[error("Failed to format JSON for file {}.", .path.style(Style::Path))]
    WriteFile {
        path: PathBuf,
        #[source]
        error: serde_json::Error,
    },
}
