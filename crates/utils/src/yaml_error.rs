use crate::fs::FsError;
use starbase_styles::{Style, Stylize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
#[cfg_attr(feature = "miette", derive(miette::Diagnostic))]
pub enum YamlError {
    #[cfg_attr(feature = "miette", diagnostic(transparent))]
    #[error(transparent)]
    Fs(#[from] FsError),

    #[cfg_attr(feature = "miette", diagnostic(code(yaml::format)))]
    #[error("Failed to format YAML.")]
    Format {
        #[source]
        error: serde_yaml::Error,
    },

    #[cfg_attr(feature = "miette", diagnostic(code(yaml::parse)))]
    #[error("Failed to parse YAML.")]
    Parse {
        #[source]
        error: serde_yaml::Error,
    },

    #[cfg_attr(feature = "miette", diagnostic(code(yaml::parse_file)))]
    #[error("Failed to parse YAML file {}.", .path.style(Style::Path))]
    ReadFile {
        path: PathBuf,
        #[source]
        error: serde_yaml::Error,
    },

    #[cfg_attr(feature = "miette", diagnostic(code(yaml::format_file)))]
    #[error("Failed to format YAML for file {}.", .path.style(Style::Path))]
    WriteFile {
        path: PathBuf,
        #[source]
        error: serde_yaml::Error,
    },
}
