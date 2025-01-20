use crate::fs::FsError;
use starbase_styles::{Style, Stylize};
use std::path::PathBuf;
use thiserror::Error;

#[cfg(not(feature = "miette"))]
#[derive(Error, Debug)]
pub enum YamlError {
    #[error(transparent)]
    Fs(#[from] Box<FsError>),

    #[error("Failed to format YAML.\n{error}")]
    Format {
        #[source]
        error: Box<serde_yml::Error>,
    },

    #[error("Failed to parse YAML.\n{error}")]
    Parse {
        #[source]
        error: Box<serde_yml::Error>,
    },

    #[error("Failed to parse YAML file {}.\n{error}", .path.style(Style::Path))]
    ReadFile {
        path: PathBuf,
        #[source]
        error: Box<serde_yml::Error>,
    },

    #[error("Failed to format YAML for file {}.\n{error}", .path.style(Style::Path))]
    WriteFile {
        path: PathBuf,
        #[source]
        error: Box<serde_yml::Error>,
    },
}

#[cfg(feature = "miette")]
#[derive(Error, Debug, miette::Diagnostic)]
pub enum YamlError {
    #[diagnostic(transparent)]
    #[error(transparent)]
    Fs(#[from] Box<FsError>),

    #[diagnostic(code(yaml::format))]
    #[error("Failed to format YAML.")]
    Format {
        #[source]
        error: Box<serde_yml::Error>,
    },

    #[diagnostic(code(yaml::parse))]
    #[error("Failed to parse YAML.")]
    Parse {
        #[source]
        error: Box<serde_yml::Error>,
    },

    #[diagnostic(code(yaml::parse_file))]
    #[error("Failed to parse YAML file {}.", .path.style(Style::Path))]
    ReadFile {
        path: PathBuf,
        #[source]
        error: Box<serde_yml::Error>,
    },

    #[diagnostic(code(yaml::format_file))]
    #[error("Failed to format YAML for file {}.", .path.style(Style::Path))]
    WriteFile {
        path: PathBuf,
        #[source]
        error: Box<serde_yml::Error>,
    },
}

impl From<FsError> for YamlError {
    fn from(e: FsError) -> YamlError {
        YamlError::Fs(Box::new(e))
    }
}
