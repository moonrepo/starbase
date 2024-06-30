use crate::fs;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fmt::Debug;
use std::path::Path;
use tracing::{instrument, trace};

pub use crate::json_error::JsonError;
pub use serde_json;
pub use serde_json::{json, Map as JsonMap, Number as JsonNumber, Value as JsonValue};

/// Clean a JSON string by removing comments and trailing commas.
#[inline]
#[instrument(name = "clean_json", skip_all)]
pub fn clean<T: AsRef<str>>(json: T) -> Result<String, std::io::Error> {
    let mut json = json.as_ref().to_owned();

    if !json.is_empty() {
        json_strip_comments::strip(&mut json)?;
    }

    Ok(json)
}

/// Recursively merge [`JsonValue`] objects, with values from next overwriting previous.
#[inline]
#[instrument(name = "merge_json", skip_all)]
pub fn merge(prev: &JsonValue, next: &JsonValue) -> JsonValue {
    match (prev, next) {
        (JsonValue::Object(prev_object), JsonValue::Object(next_object)) => {
            let mut object = prev_object.clone();

            for (key, value) in next_object.iter() {
                if let Some(prev_value) = prev_object.get(key) {
                    object.insert(key.to_owned(), merge(prev_value, value));
                } else {
                    object.insert(key.to_owned(), value.to_owned());
                }
            }

            JsonValue::Object(object)
        }
        _ => next.to_owned(),
    }
}

/// Parse a string and deserialize into the required type.
#[inline]
#[instrument(name = "parse_json", skip(data))]
pub fn parse<T, D>(data: T) -> Result<D, JsonError>
where
    T: AsRef<str>,
    D: DeserializeOwned,
{
    trace!("Parsing JSON");

    let contents = clean(data.as_ref()).map_err(|error| JsonError::Clean {
        error: Box::new(error),
    })?;

    serde_json::from_str(&contents).map_err(|error| JsonError::Parse {
        error: Box::new(error),
    })
}

/// Format and serialize the provided value into a string.
#[inline]
#[instrument(name = "format_json", skip(data))]
pub fn format<D>(data: &D, pretty: bool) -> Result<String, JsonError>
where
    D: ?Sized + Serialize,
{
    trace!("Formatting JSON");

    if pretty {
        serde_json::to_string_pretty(&data).map_err(|error| JsonError::Format {
            error: Box::new(error),
        })
    } else {
        serde_json::to_string(&data).map_err(|error| JsonError::Format {
            error: Box::new(error),
        })
    }
}

/// Format and serialize the provided value into a string, with the provided
/// indentation. This can be used to preserve the original indentation of a file.
#[inline]
#[instrument(name = "format_json_with_identation", skip(data))]
pub fn format_with_identation<D>(data: &D, indent: &str) -> Result<String, JsonError>
where
    D: ?Sized + Serialize,
{
    use serde_json::ser::PrettyFormatter;
    use serde_json::Serializer;

    trace!(indent, "Formatting JSON with preserved indentation");

    // Based on serde_json::to_string_pretty!
    let mut writer = Vec::with_capacity(128);
    let mut serializer =
        Serializer::with_formatter(&mut writer, PrettyFormatter::with_indent(indent.as_bytes()));

    data.serialize(&mut serializer)
        .map_err(|error| JsonError::Format {
            error: Box::new(error),
        })?;

    Ok(unsafe { String::from_utf8_unchecked(writer) })
}

/// Read a file at the provided path and deserialize into the required type.
/// The path must already exist.
#[inline]
#[instrument(name = "read_json")]
pub fn read_file<P, D>(path: P) -> Result<D, JsonError>
where
    P: AsRef<Path> + Debug,
    D: DeserializeOwned,
{
    let path = path.as_ref();
    let contents = clean(fs::read_file(path)?).map_err(|error| JsonError::CleanFile {
        path: path.to_owned(),
        error: Box::new(error),
    })?;

    trace!(file = ?path, "Reading JSON file");

    serde_json::from_str(&contents).map_err(|error| JsonError::ReadFile {
        path: path.to_path_buf(),
        error: Box::new(error),
    })
}

/// Write a file and serialize the provided data to the provided path. If the parent directory
/// does not exist, it will be created.
///
/// This function is primarily used internally for non-consumer facing files.
#[inline]
#[instrument(name = "write_json", skip(json))]
pub fn write_file<P, D>(path: P, json: &D, pretty: bool) -> Result<(), JsonError>
where
    P: AsRef<Path> + Debug,
    D: ?Sized + Serialize,
{
    let path = path.as_ref();

    trace!(file = ?path, "Writing JSON file");

    let data = if pretty {
        serde_json::to_string_pretty(&json).map_err(|error| JsonError::WriteFile {
            path: path.to_path_buf(),
            error: Box::new(error),
        })?
    } else {
        serde_json::to_string(&json).map_err(|error| JsonError::WriteFile {
            path: path.to_path_buf(),
            error: Box::new(error),
        })?
    };

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
#[instrument(name = "write_json_with_config", skip(json))]
pub fn write_file_with_config<P, D>(path: P, json: &D, pretty: bool) -> Result<(), JsonError>
where
    P: AsRef<Path> + Debug,
    D: ?Sized + Serialize,
{
    if !pretty {
        return write_file(path, &json, false);
    }

    trace!(file = ?path, "Writing JSON file with .editorconfig");

    let path = path.as_ref();
    let editor_config = fs::get_editor_config_props(path)?;

    let mut data = format_with_identation(&json, &editor_config.indent)?;
    editor_config.apply_eof(&mut data);

    fs::write_file(path, data)?;

    Ok(())
}
