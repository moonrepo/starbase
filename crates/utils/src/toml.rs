use crate::fs::{self, FsError};
use miette::Diagnostic;
use serde::de::DeserializeOwned;
use serde::Serialize;
use starbase_styles::{Style, Stylize};
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::trace;

pub use toml as serde_toml;
pub use toml::{
    from_str, to_string, to_string_pretty,
    value::{Datetime as TomlDatetime, Table as TomlTable, Value as TomlValue},
};

#[derive(Error, Diagnostic, Debug)]
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

/// Read a file at the provided path and deserialize into the required type.
/// The path must already exist.
#[inline]
pub fn read_file<P, D>(path: P) -> Result<D, TomlError>
where
    P: AsRef<Path>,
    D: DeserializeOwned,
{
    let path = path.as_ref();
    let contents = fs::read_file(path)?;

    trace!(file = ?path, "Parsing TOML");

    toml::from_str(&contents).map_err(|error| TomlError::ReadFile {
        path: path.to_path_buf(),
        error,
    })
}

/// Write a file and serialize the provided data to the provided path. If the parent directory
/// does not exist, it will be created.
#[inline]
pub fn write_file<P, D>(path: P, toml: &D, pretty: bool) -> Result<(), TomlError>
where
    P: AsRef<Path>,
    D: ?Sized + Serialize,
{
    let path = path.as_ref();

    trace!(file = ?path, "Stringifying TOML");

    let data = if pretty {
        toml::to_string_pretty(&toml).map_err(|error| TomlError::StringifyFile {
            path: path.to_path_buf(),
            error,
        })?
    } else {
        toml::to_string(&toml).map_err(|error| TomlError::StringifyFile {
            path: path.to_path_buf(),
            error,
        })?
    };

    fs::write_file(path, data)?;

    Ok(())
}
