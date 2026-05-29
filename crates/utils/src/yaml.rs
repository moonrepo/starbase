use crate::fs;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fmt::Debug;
use std::path::Path;
use tracing::{instrument, trace};

pub use crate::yaml_error::YamlError;
pub use serde_norway as serde_yaml;
pub use serde_norway::{
    Mapping as YamlMapping, Number as YamlNumber, Sequence as YamlSequence, Value as YamlValue,
};

/// Recursively merge [`YamlValue`] objects, with values from next overwriting previous.
#[inline]
#[instrument(name = "merge_yaml", skip_all)]
pub fn merge(prev: &YamlValue, next: &YamlValue) -> YamlValue {
    let mut merged = prev.to_owned();

    merge_into(&mut merged, next);

    merged
}

fn merge_into(prev: &mut YamlValue, next: &YamlValue) {
    match (prev, next) {
        (YamlValue::Mapping(prev_object), YamlValue::Mapping(next_object)) => {
            for (key, value) in next_object {
                if let Some(prev_value) = prev_object.get_mut(key) {
                    merge_into(prev_value, value);
                } else {
                    prev_object.insert(key.to_owned(), value.to_owned());
                }
            }
        }
        (prev, next) => *prev = next.to_owned(),
    }
}

/// Parse a string and deserialize into the required type.
#[inline]
#[instrument(name = "parse_yaml", skip(data))]
pub fn parse<D>(data: impl AsRef<str>) -> Result<D, YamlError>
where
    D: DeserializeOwned,
{
    trace!("Parsing YAML");

    serde_norway::from_str(data.as_ref()).map_err(|error| YamlError::Parse {
        error: Box::new(error),
    })
}

/// Format and serialize the provided value into a string.
#[inline]
#[instrument(name = "format_yaml", skip(data))]
pub fn format<D>(data: &D) -> Result<String, YamlError>
where
    D: ?Sized + Serialize,
{
    trace!("Formatting YAML");

    serde_norway::to_string(&data).map_err(|error| YamlError::Format {
        error: Box::new(error),
    })
}

/// Format and serialize the provided value into a string, with the provided
/// indentation. This can be used to preserve the original indentation of a file.
#[inline]
#[instrument(name = "format_yaml_with_identation", skip(data))]
pub fn format_with_identation<D>(data: &D, indent: &str) -> Result<String, YamlError>
where
    D: ?Sized + Serialize,
{
    trace!("Formatting YAML with preserved indentation");

    let data = serde_norway::to_string(data).map_err(|error| YamlError::Format {
        error: Box::new(error),
    })?;
    let data = data.trim();

    // serde does not support customizing the indentation character. So to work around
    // this, we do it manually on the YAML string, but only if the indent is different than
    // a double space (the default), which can be customized with `.editorconfig`.
    if indent != "  " {
        return Ok(format_with_custom_indentation(data, indent));
    }

    Ok(data.to_owned())
}

fn format_with_custom_indentation(data: &str, indent: &str) -> String {
    let mut output = String::with_capacity(data.len());

    for (index, line) in data.split('\n').enumerate() {
        if index > 0 {
            output.push('\n');
        }

        if !line.starts_with("  ") {
            output.push_str(line);
            continue;
        }

        let leading_spaces = line
            .as_bytes()
            .iter()
            .take_while(|char| **char == b' ')
            .count();

        for _ in 0..(leading_spaces / 2) {
            output.push_str(indent);
        }

        output.push_str(&line[leading_spaces..]);
    }

    output
}

/// Read a file at the provided path and deserialize into the required type.
/// The path must already exist.
#[inline]
#[instrument(name = "read_yaml")]
pub fn read_file<D>(path: impl AsRef<Path> + Debug) -> Result<D, YamlError>
where
    D: DeserializeOwned,
{
    let path = path.as_ref();
    let contents = fs::read_file(path)?;

    trace!(file = ?path, "Reading YAML file");

    serde_norway::from_str(&contents).map_err(|error| YamlError::ReadFile {
        path: path.to_path_buf(),
        error: Box::new(error),
    })
}

/// Write a file and serialize the provided data to the provided path. If the parent directory
/// does not exist, it will be created.
///
/// This function is primarily used internally for non-consumer facing files.
#[inline]
#[instrument(name = "write_yaml", skip(data))]
pub fn write_file<D>(path: impl AsRef<Path> + Debug, data: &D) -> Result<(), YamlError>
where
    D: ?Sized + Serialize,
{
    let path = path.as_ref();

    trace!(file = ?path, "Writing YAML file");

    let data = serde_norway::to_string(&data).map_err(|error| YamlError::WriteFile {
        path: path.to_path_buf(),
        error: Box::new(error),
    })?;

    fs::write_file(path, data)?;

    Ok(())
}

/// Write a file and serialize the provided data to the provided path, while taking the
/// closest `.editorconfig` into account. If the parent directory does not exist,
/// it will be created.
///
/// This function is used for consumer facing files, like configs.
#[cfg(feature = "editor-config")]
#[inline]
#[instrument(name = "write_yaml_with_config", skip(data))]
pub fn write_file_with_config<D>(path: impl AsRef<Path> + Debug, data: &D) -> Result<(), YamlError>
where
    D: ?Sized + Serialize,
{
    trace!(file = ?path, "Writing YAML file with .editorconfig");

    let path = path.as_ref();
    let editor_config = fs::get_editor_config_props(path)?;

    let mut data = format_with_identation(data, &editor_config.indent)?;
    editor_config.apply_eof(&mut data);

    fs::write_file(path, data)?;

    Ok(())
}
