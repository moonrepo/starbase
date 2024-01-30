use crate::fs;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::path::Path;
use tracing::trace;

pub use crate::toml_error::TomlError;
pub use toml as serde_toml;
pub use toml::{
    from_str, to_string, to_string_pretty,
    value::{Datetime as TomlDatetime, Table as TomlTable, Value as TomlValue},
};

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
