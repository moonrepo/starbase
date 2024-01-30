use crate::fs::FsError;
use starbase_styles::{Style, Stylize};
use std::path::PathBuf;
use thiserror::Error;

#[cfg(not(feature = "miette"))]
#[derive(Error, Debug)]
pub enum YamlError {
    #[error(transparent)]
    Fs(#[from] FsError),

    #[error("Failed to parse YAML file {}.\n{error}", .path.style(Style::Path))]
    ReadFile {
        path: PathBuf,
        #[source]
        error: serde_yaml::Error,
    },

    #[error("Failed to stringify YAML.\n{error}")]
    Stringify {
        #[source]
        error: serde_yaml::Error,
    },

    #[error("Failed to stringify YAML for file {}.\n{error}", .path.style(Style::Path))]
    StringifyFile {
        path: PathBuf,
        #[source]
        error: serde_yaml::Error,
    },
}

#[cfg(feature = "miette")]
#[derive(Error, Debug, miette::Diagnostic)]
pub enum YamlError {
    #[diagnostic(transparent)]
    #[error(transparent)]
    Fs(#[from] FsError),

    #[diagnostic(code(yaml::parse_file))]
    #[error("Failed to parse YAML file {}.", .path.style(Style::Path))]
    ReadFile {
        path: PathBuf,
        #[source]
        error: serde_yaml::Error,
    },

    #[diagnostic(code(yaml::stringify))]
    #[error("Failed to stringify YAML.")]
    Stringify {
        #[source]
        error: serde_yaml::Error,
    },

    #[diagnostic(code(yaml::stringify_file))]
    #[error("Failed to stringify YAML for file {}.", .path.style(Style::Path))]
    StringifyFile {
        path: PathBuf,
        #[source]
        error: serde_yaml::Error,
    },
}
