use crate::fs::FsError;
use starbase_styles::{Style, Stylize};
use std::path::PathBuf;
use thiserror::Error;

#[cfg(not(feature = "miette"))]
#[derive(Error, Debug)]
pub enum JsonError {
    #[error(transparent)]
    Fs(#[from] FsError),

    #[error("Failed to clean comments and trailing commas in JSON file {}.\n{error}", .path.style(Style::Path))]
    Clean {
        path: PathBuf,
        #[source]
        error: std::io::Error,
    },

    #[error("Failed to parse JSON file {}.\n{error}", .path.style(Style::Path))]
    ReadFile {
        path: PathBuf,
        #[source]
        error: serde_json::Error,
    },

    #[error("Failed to stringify JSON.\n{error}")]
    Stringify {
        #[source]
        error: serde_json::Error,
    },

    #[error("Failed to stringify JSON for file {}.\n{error}", .path.style(Style::Path))]
    StringifyFile {
        path: PathBuf,
        #[source]
        error: serde_json::Error,
    },
}

#[cfg(feature = "miette")]
#[derive(Error, Debug, miette::Diagnostic)]
pub enum JsonError {
    #[diagnostic(transparent)]
    #[error(transparent)]
    Fs(#[from] FsError),

    #[diagnostic(code(json::clean))]
    #[error("Failed to clean comments and trailing commas in JSON file {}.", .path.style(Style::Path))]
    Clean {
        path: PathBuf,
        #[source]
        error: std::io::Error,
    },

    #[diagnostic(code(json::parse_file))]
    #[error("Failed to parse JSON file {}.", .path.style(Style::Path))]
    ReadFile {
        path: PathBuf,
        #[source]
        error: serde_json::Error,
    },

    #[diagnostic(code(json::stringify))]
    #[error("Failed to stringify JSON.")]
    Stringify {
        #[source]
        error: serde_json::Error,
    },

    #[diagnostic(code(json::stringify_file))]
    #[error("Failed to stringify JSON for file {}.", .path.style(Style::Path))]
    StringifyFile {
        path: PathBuf,
        #[source]
        error: serde_json::Error,
    },
}
