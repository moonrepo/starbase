use crate::fs::FsError;
use starbase_styles::{Style, Stylize};
use std::path::PathBuf;
use thiserror::Error;

#[cfg(not(feature = "miette"))]
#[derive(Error, Debug)]
pub enum TomlError {
    #[error(transparent)]
    Fs(#[from] Box<FsError>),

    #[error("Failed to format TOML.\n{error}")]
    Format {
        #[source]
        error: Box<toml::ser::Error>,
    },

    #[error("Failed to parse TOML.\n{error}")]
    Parse {
        #[source]
        error: Box<toml::de::Error>,
    },

    #[error("Failed to parse TOML file {}.\n{error}", .path.style(Style::Path))]
    ReadFile {
        path: PathBuf,
        #[source]
        error: Box<toml::de::Error>,
    },

    #[error("Failed to format TOML for file {}.\n{error}", .path.style(Style::Path))]
    WriteFile {
        path: PathBuf,
        #[source]
        error: Box<toml::ser::Error>,
    },
}

#[cfg(feature = "miette")]
#[derive(Error, Debug, miette::Diagnostic)]
pub enum TomlError {
    #[diagnostic(transparent)]
    #[error(transparent)]
    Fs(#[from] Box<FsError>),

    #[diagnostic(code(toml::format))]
    #[error("Failed to format TOML.")]
    Format {
        #[source]
        error: Box<toml::ser::Error>,
    },

    #[diagnostic(code(toml::parse))]
    #[error("Failed to parse TOML.")]
    Parse {
        #[source]
        error: Box<toml::de::Error>,
    },

    #[diagnostic(code(toml::parse_file))]
    #[error("Failed to parse TOML file {}.", .path.style(Style::Path))]
    ReadFile {
        path: PathBuf,
        #[source]
        error: Box<toml::de::Error>,
    },

    #[diagnostic(code(toml::format_file))]
    #[error("Failed to format TOML for file {}.", .path.style(Style::Path))]
    WriteFile {
        path: PathBuf,
        #[source]
        error: Box<toml::ser::Error>,
    },
}

impl From<FsError> for TomlError {
    fn from(e: FsError) -> TomlError {
        TomlError::Fs(Box::new(e))
    }
}
