use crate::fs;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::path::Path;
use tracing::trace;

pub use crate::yaml_error::YamlError;
pub use serde_yaml;
pub use serde_yaml::{
    Mapping as YamlMapping, Number as YamlNumber, Sequence as YamlSequence, Value as YamlValue,
};

static WHITESPACE_PREFIX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(\s+)").unwrap());

/// Recursively merge [`YamlValue`] objects, with values from next overwriting previous.
#[inline]
pub fn merge(prev: &YamlValue, next: &YamlValue) -> YamlValue {
    match (prev, next) {
        (YamlValue::Mapping(prev_object), YamlValue::Mapping(next_object)) => {
            let mut object = prev_object.clone();

            for (key, value) in next_object.iter() {
                if let Some(prev_value) = prev_object.get(key) {
                    object.insert(key.to_owned(), merge(prev_value, value));
                } else {
                    object.insert(key.to_owned(), value.to_owned());
                }
            }

            YamlValue::Mapping(object)
        }
        _ => next.to_owned(),
    }
}

/// Parse a string and deserialize into the required type.
#[inline]
pub fn parse<T, D>(data: T) -> Result<D, YamlError>
where
    T: AsRef<str>,
    D: DeserializeOwned,
{
    trace!("Parsing YAML");

    serde_yaml::from_str(data.as_ref()).map_err(|error| YamlError::Parse { error })
}

/// Format and serialize the provided value into a string.
#[inline]
pub fn format<D>(data: &D) -> Result<String, YamlError>
where
    D: ?Sized + Serialize,
{
    trace!("Formatting YAML");

    serde_yaml::to_string(&data).map_err(|error| YamlError::Format { error })
}

/// Read a file at the provided path and deserialize into the required type.
/// The path must already exist.
#[inline]
pub fn read_file<P, D>(path: P) -> Result<D, YamlError>
where
    P: AsRef<Path>,
    D: DeserializeOwned,
{
    let path = path.as_ref();
    let contents = fs::read_file(path)?;

    trace!(file = ?path, "Reading YAML file");

    serde_yaml::from_str(&contents).map_err(|error| YamlError::ReadFile {
        path: path.to_path_buf(),
        error,
    })
}

/// Write a file and serialize the provided data to the provided path. If the parent directory
/// does not exist, it will be created.
///
/// This function is primarily used internally for non-consumer facing files.
#[inline]
pub fn write_file<P, D>(path: P, yaml: &D) -> Result<(), YamlError>
where
    P: AsRef<Path>,
    D: ?Sized + Serialize,
{
    let path = path.as_ref();

    trace!(file = ?path, "Writing YAML file");

    let data = serde_yaml::to_string(&yaml).map_err(|error| YamlError::WriteFile {
        path: path.to_path_buf(),
        error,
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
pub fn write_file_with_config<P, D>(path: P, yaml: &D) -> Result<(), YamlError>
where
    P: AsRef<Path>,
    D: ?Sized + Serialize,
{
    let path = path.as_ref();
    let editor_config = fs::get_editor_config_props(path);

    trace!(file = ?path, "Writing YAML file with .editorconfig");

    let mut data = serde_yaml::to_string(&yaml)
        .map_err(|error| YamlError::WriteFile {
            path: path.to_path_buf(),
            error,
        })?
        .trim()
        .to_string();

    // serde_yaml does not support customizing the indentation character. So to work around
    // this, we do it manually on the YAML string, but only if the indent is different than
    // a double space (the default), which can be customized with `.editorconfig`.
    if editor_config.indent != "  " {
        data = data
            .split('\n')
            .map(|line| {
                if !line.starts_with("  ") {
                    return line.to_string();
                }

                WHITESPACE_PREFIX
                    .replace_all(line, |caps: &regex::Captures| {
                        editor_config
                            .indent
                            .repeat(caps.get(1).unwrap().as_str().len() / 2)
                    })
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n");
    }

    editor_config.apply_eof(&mut data);

    fs::write_file(path, data)?;

    Ok(())
}
