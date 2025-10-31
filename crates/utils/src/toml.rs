use crate::fs;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fmt::Debug;
use std::path::Path;
use tracing::{instrument, trace};

pub use crate::toml_error::TomlError;
pub use toml as serde_toml;
pub use toml::value::{Datetime as TomlDatetime, Table as TomlTable, Value as TomlValue};

/// Parse a string and deserialize into the required type.
#[inline]
#[instrument(name = "parse_toml", skip(data))]
pub fn parse<D>(data: impl AsRef<str>) -> Result<D, TomlError>
where
    D: DeserializeOwned,
{
    trace!("Parsing TOML");

    toml::from_str(data.as_ref()).map_err(|error| TomlError::Parse {
        error: Box::new(error),
    })
}

/// Format and serialize the provided value into a string.
#[inline]
#[instrument(name = "format_toml", skip(data))]
pub fn format<D>(data: &D, pretty: bool) -> Result<String, TomlError>
where
    D: ?Sized + Serialize,
{
    trace!("Formatting TOML");

    if pretty {
        toml::to_string_pretty(&data).map_err(|error| TomlError::Format {
            error: Box::new(error),
        })
    } else {
        toml::to_string(&data).map_err(|error| TomlError::Format {
            error: Box::new(error),
        })
    }
}

/// Read a file at the provided path and deserialize into the required type.
/// The path must already exist.
#[inline]
#[instrument(name = "read_toml")]
pub fn read_file<D>(path: impl AsRef<Path> + Debug) -> Result<D, TomlError>
where
    D: DeserializeOwned,
{
    let path = path.as_ref();
    let contents = fs::read_file(path)?;

    trace!(file = ?path, "Reading TOML file");

    toml::from_str(&contents).map_err(|error| TomlError::ReadFile {
        path: path.to_path_buf(),
        error: Box::new(error),
    })
}

/// Write a file and serialize the provided data to the provided path. If the parent directory
/// does not exist, it will be created.
#[inline]
#[instrument(name = "write_toml", skip(data))]
pub fn write_file<D>(
    path: impl AsRef<Path> + Debug,
    data: &D,
    pretty: bool,
) -> Result<(), TomlError>
where
    D: ?Sized + Serialize,
{
    let path = path.as_ref();

    trace!(file = ?path, "Writing TOML file");

    let data = if pretty {
        toml::to_string_pretty(&data).map_err(|error| TomlError::WriteFile {
            path: path.to_path_buf(),
            error: Box::new(error),
        })?
    } else {
        toml::to_string(&data).map_err(|error| TomlError::WriteFile {
            path: path.to_path_buf(),
            error: Box::new(error),
        })?
    };

    fs::write_file(path, data)?;

    Ok(())
}
