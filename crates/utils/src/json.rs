use crate::fs;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::path::Path;
use tracing::trace;

pub use crate::json_error::JsonError;
pub use serde_json;
pub use serde_json::{json, Map as JsonMap, Number as JsonNumber, Value as JsonValue};

/// Clean a JSON string by removing comments and trailing commas.
#[inline]
pub fn clean<T: AsRef<str>>(json: T) -> Result<String, std::io::Error> {
    let mut json = json.as_ref().to_owned();

    if !json.is_empty() {
        json_strip_comments::strip(&mut json)?;
    }

    Ok(json)
}

/// Recursively merge [`JsonValue`] objects, with values from next overwriting previous.
#[inline]
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
pub fn parse<T, D>(data: T) -> Result<D, JsonError>
where
    T: AsRef<str>,
    D: DeserializeOwned,
{
    trace!("Parsing JSON");

    let contents = clean(data.as_ref()).map_err(|error| JsonError::Clean { error })?;

    serde_json::from_str(&contents).map_err(|error| JsonError::Parse { error })
}

/// Format and serialize the provided value into a string.
#[inline]
pub fn format<D>(data: &D, pretty: bool) -> Result<String, JsonError>
where
    D: ?Sized + Serialize,
{
    trace!("Formatting JSON");

    if pretty {
        serde_json::to_string_pretty(&data).map_err(|error| JsonError::Format { error })
    } else {
        serde_json::to_string(&data).map_err(|error| JsonError::Format { error })
    }
}

/// Read a file at the provided path and deserialize into the required type.
/// The path must already exist.
#[inline]
pub fn read_file<P, D>(path: P) -> Result<D, JsonError>
where
    P: AsRef<Path>,
    D: DeserializeOwned,
{
    let path = path.as_ref();
    let contents = clean(fs::read_file(path)?).map_err(|error| JsonError::CleanFile {
        path: path.to_owned(),
        error,
    })?;

    trace!(file = ?path, "Reading JSON file");

    serde_json::from_str(&contents).map_err(|error| JsonError::ReadFile {
        path: path.to_path_buf(),
        error,
    })
}

/// Write a file and serialize the provided data to the provided path. If the parent directory
/// does not exist, it will be created.
///
/// This function is primarily used internally for non-consumer facing files.
#[inline]
pub fn write_file<P, D>(path: P, json: &D, pretty: bool) -> Result<(), JsonError>
where
    P: AsRef<Path>,
    D: ?Sized + Serialize,
{
    let path = path.as_ref();

    trace!(file = ?path, "Writing JSON file");

    let data = if pretty {
        serde_json::to_string_pretty(&json).map_err(|error| JsonError::WriteFile {
            path: path.to_path_buf(),
            error,
        })?
    } else {
        serde_json::to_string(&json).map_err(|error| JsonError::WriteFile {
            path: path.to_path_buf(),
            error,
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
pub fn write_file_with_config<P: AsRef<Path>>(
    path: P,
    json: JsonValue,
    pretty: bool,
) -> Result<(), JsonError> {
    if !pretty {
        return write_file(path, &json, false);
    }

    use serde_json::ser::PrettyFormatter;
    use serde_json::Serializer;

    let path = path.as_ref();
    let editor_config = fs::get_editor_config_props(path);

    trace!(file = ?path, "Writing JSON file with .editorconfig");

    // Based on serde_json::to_string_pretty!
    let mut writer = Vec::with_capacity(128);
    let mut serializer = Serializer::with_formatter(
        &mut writer,
        PrettyFormatter::with_indent(editor_config.indent.as_bytes()),
    );

    json.serialize(&mut serializer)
        .map_err(|error| JsonError::WriteFile {
            path: path.to_path_buf(),
            error,
        })?;

    let mut data = unsafe { String::from_utf8_unchecked(writer) };
    editor_config.apply_eof(&mut data);

    fs::write_file(path, data)?;

    Ok(())
}
