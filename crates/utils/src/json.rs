use crate::fs::{self, FsError};
use json_comments::StripComments;
use miette::Diagnostic;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::de::DeserializeOwned;
use serde::Serialize;
use starbase_styles::{Style, Stylize};
use std::path::Path;
use std::{io::Read, path::PathBuf};
use thiserror::Error;
use tracing::trace;

pub use serde_json::{
    from_str, from_value, json, to_string, to_string_pretty, to_value, Map as JsonMap,
    Number as JsonNumber, Value as JsonValue,
};

#[derive(Error, Diagnostic, Debug)]
pub enum JsonError {
    #[diagnostic(transparent)]
    #[error(transparent)]
    Fs(#[from] FsError),

    #[diagnostic(code(json::parse_file))]
    #[error("Failed to parse JSON file {}", .path.style(Style::Path))]
    ReadFile {
        path: PathBuf,
        #[source]
        error: serde_json::Error,
    },

    #[diagnostic(code(json::stringify))]
    #[error("Failed to stringify JSON")]
    Stringify {
        #[source]
        error: serde_json::Error,
    },

    #[diagnostic(code(json::stringify_file))]
    #[error("Failed to stringify JSON for file {}", .path.style(Style::Path))]
    StringifyFile {
        path: PathBuf,
        #[source]
        error: serde_json::Error,
    },
}

static CLEAN_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r",(?P<valid>\s*})").unwrap());

/// Clean a JSON string by removing comments and trailing commas.
#[inline]
#[track_caller]
pub fn clean<D: AsRef<str>>(json: D) -> String {
    let json = json.as_ref();

    // Remove comments
    let mut stripped = String::with_capacity(json.len());

    StripComments::new(json.as_bytes())
        .read_to_string(&mut stripped)
        .unwrap();

    // Remove trailing commas
    CLEAN_REGEX.replace_all(&stripped, "$valid").to_string()
}

/// Recursively merge [JsonValue] objects, with values from next overwriting previous.
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

/// Read a file at the provided path and deserialize into the required type.
/// The path must already exist.
#[inline]
pub fn read_file<P, D>(path: P) -> Result<D, JsonError>
where
    P: AsRef<Path>,
    D: DeserializeOwned,
{
    let path = path.as_ref();
    let contents = read_to_string(path)?;

    trace!(file = %path.display(), "Parsing JSON");

    serde_json::from_str(&contents).map_err(|error| JsonError::ReadFile {
        path: path.to_path_buf(),
        error,
    })
}

/// Read a file at the provided path into a string, without deserializing it.
/// The path must already exist.
#[inline]
pub fn read_to_string<T: AsRef<Path>>(path: T) -> Result<String, JsonError> {
    Ok(clean(fs::read_file(path.as_ref())?))
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

    trace!(file = %path.display(), "Stringifying JSON");

    let data = if pretty {
        serde_json::to_string_pretty(&json).map_err(|error| JsonError::StringifyFile {
            path: path.to_path_buf(),
            error,
        })?
    } else {
        serde_json::to_string(&json).map_err(|error| JsonError::StringifyFile {
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
pub fn write_with_config<P: AsRef<Path>>(
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

    trace!(file = %path.display(), "Stringifying JSON with .editorconfig");

    // Based on serde_json::to_string_pretty!
    let mut writer = Vec::with_capacity(128);
    let mut serializer = Serializer::with_formatter(
        &mut writer,
        PrettyFormatter::with_indent(editor_config.indent.as_bytes()),
    );

    json.serialize(&mut serializer)
        .map_err(|error| JsonError::StringifyFile {
            path: path.to_path_buf(),
            error,
        })?;

    let mut data = unsafe { String::from_utf8_unchecked(writer) };
    editor_config.apply_eof(&mut data);

    fs::write_file(path, data)?;

    Ok(())
}
