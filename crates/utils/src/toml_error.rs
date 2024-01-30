use crate::fs::FsError;
use starbase_styles::{Style, Stylize};
use std::path::PathBuf;
use thiserror::Error;

#[cfg(not(feature = "miette"))]
#[derive(Error, Debug)]
pub enum TomlError {
    #[error(transparent)]
    Fs(#[from] FsError),

    #[error("Failed to parse TOML file {}.\n{error}", .path.style(Style::Path))]
    ReadFile {
        path: PathBuf,
        #[source]
        error: toml::de::Error,
    },

    #[error("Failed to stringify TOML.\n{error}")]
    Stringify {
        #[source]
        error: toml::ser::Error,
    },

    #[error("Failed to stringify TOML for file {}.\n{error}", .path.style(Style::Path))]
    StringifyFile {
        path: PathBuf,
        #[source]
        error: toml::ser::Error,
    },
}

#[cfg(feature = "miette")]
#[derive(Error, Debug, miette::Diagnostic)]
pub enum TomlError {
    #[diagnostic(transparent)]
    #[error(transparent)]
    Fs(#[from] FsError),

    #[diagnostic(code(toml::parse_file))]
    #[error("Failed to parse TOML file {}.", .path.style(Style::Path))]
    ReadFile {
        path: PathBuf,
        #[source]
        error: toml::de::Error,
    },

    #[diagnostic(code(toml::stringify))]
    #[error("Failed to stringify TOML.")]
    Stringify {
        #[source]
        error: toml::ser::Error,
    },

    #[diagnostic(code(toml::stringify_file))]
    #[error("Failed to stringify TOML for file {}.", .path.style(Style::Path))]
    StringifyFile {
        path: PathBuf,
        #[source]
        error: toml::ser::Error,
    },
}
