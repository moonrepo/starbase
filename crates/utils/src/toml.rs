use crate::fs::{self, FsError};
use miette::Diagnostic;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub use toml::value;

#[derive(Error, Diagnostic, Debug)]
pub enum TomlError {
    #[error(transparent)]
    Fs(#[from] FsError),

    #[diagnostic(code(toml::parse_file))]
    #[error("Failed to parse TOML file <path>{path}</path>")]
    ReadFile {
        path: PathBuf,
        #[source]
        error: toml::de::Error,
    },

    #[diagnostic(code(toml::stringify_file))]
    #[error("Failed to stringify TOML for file <path>{path}</path>")]
    StringifyFile {
        path: PathBuf,
        #[source]
        error: toml::ser::Error,
    },
}

#[inline]
pub fn read_file<P, D>(path: P) -> Result<D, TomlError>
where
    P: AsRef<Path>,
    D: DeserializeOwned,
{
    let path = path.as_ref();
    let contents = fs::read_file(path)?;

    toml::from_str(&contents).map_err(|error| TomlError::ReadFile {
        path: path.to_path_buf(),
        error,
    })
}

#[inline]
pub fn write_file<P, D>(path: P, toml: &D, pretty: bool) -> Result<(), TomlError>
where
    P: AsRef<Path>,
    D: ?Sized + Serialize,
{
    let path = path.as_ref();

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
