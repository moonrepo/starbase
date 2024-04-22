use crate::fs::FsError;
use starbase_styles::{Style, Stylize};
use std::path::PathBuf;
use thiserror::Error;

#[cfg(not(feature = "miette"))]
#[derive(Error, Debug)]
pub enum JsonError {
    #[error(transparent)]
    Fs(#[from] Box<FsError>),

    #[error("Failed to clean comments and trailing commas from JSON.\n{error}")]
    Clean {
        #[source]
        error: Box<std::io::Error>,
    },

    #[error("Failed to clean comments and trailing commas in JSON file {}.\n{error}", .path.style(Style::Path))]
    CleanFile {
        path: PathBuf,
        #[source]
        error: Box<std::io::Error>,
    },

    #[error("Failed to format JSON.\n{error}")]
    Format {
        #[source]
        error: Box<serde_json::Error>,
    },

    #[error("Failed to parse JSON.\n{error}")]
    Parse {
        #[source]
        error: Box<serde_json::Error>,
    },

    #[error("Failed to parse JSON file {}.\n{error}", .path.style(Style::Path))]
    ReadFile {
        path: PathBuf,
        #[source]
        error: Box<serde_json::Error>,
    },

    #[error("Failed to format JSON for file {}.\n{error}", .path.style(Style::Path))]
    WriteFile {
        path: PathBuf,
        #[source]
        error: Box<serde_json::Error>,
    },
}

#[cfg(feature = "miette")]
#[derive(Error, Debug, miette::Diagnostic)]
pub enum JsonError {
    #[diagnostic(transparent)]
    #[error(transparent)]
    Fs(#[from] Box<FsError>),

    #[diagnostic(code(json::clean))]
    #[error("Failed to clean comments and trailing commas from JSON.")]
    Clean {
        #[source]
        error: Box<std::io::Error>,
    },

    #[diagnostic(code(json::clean_file))]
    #[error("Failed to clean comments and trailing commas in JSON file {}.", .path.style(Style::Path))]
    CleanFile {
        path: PathBuf,
        #[source]
        error: Box<std::io::Error>,
    },

    #[diagnostic(code(json::format))]
    #[error("Failed to format JSON.")]
    Format {
        #[source]
        error: Box<serde_json::Error>,
    },

    #[diagnostic(code(json::parse))]
    #[error("Failed to parse JSON.")]
    Parse {
        #[source]
        error: Box<serde_json::Error>,
    },

    #[diagnostic(code(json::parse_file))]
    #[error("Failed to parse JSON file {}.", .path.style(Style::Path))]
    ReadFile {
        path: PathBuf,
        #[source]
        error: Box<serde_json::Error>,
    },

    #[diagnostic(code(json::format_file))]
    #[error("Failed to format JSON for file {}.", .path.style(Style::Path))]
    WriteFile {
        path: PathBuf,
        #[source]
        error: Box<serde_json::Error>,
    },
}
