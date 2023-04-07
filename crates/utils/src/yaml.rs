use crate::fs::{self, FsError};
use miette::Diagnostic;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use thiserror::Error;

pub use serde_yaml::{
    from_value, to_value, Mapping as YamlMapping, Number as YamlNumber, Sequence as YamlSequence,
    Value as YamlValue,
};

#[derive(Error, Diagnostic, Debug)]
pub enum YamlError {
    #[error(transparent)]
    Fs(#[from] FsError),

    #[diagnostic(code(yaml::read_file))]
    #[error("Failed to read YAML file <path>{path}</path>")]
    ReadFile {
        path: PathBuf,
        #[source]
        error: serde_yaml::Error,
    },

    #[diagnostic(code(yaml::stringify_file))]
    #[error("Failed to stringify YAML for file <path>{path}</path>")]
    StringifyFile {
        path: PathBuf,
        #[source]
        error: serde_yaml::Error,
    },
}

static WHITESPACE_PREFIX: Lazy<Mutex<Regex>> =
    Lazy::new(|| Mutex::new(Regex::new(r"^(\s+)").unwrap()));

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

#[inline]
pub fn read<P, D>(path: P) -> Result<D, YamlError>
where
    P: AsRef<Path>,
    D: DeserializeOwned,
{
    let path = path.as_ref();
    let contents = fs::read(path)?;

    serde_yaml::from_str(&contents).map_err(|error| YamlError::ReadFile {
        path: path.to_path_buf(),
        error,
    })
}

// This function is primarily used internally for non-consumer facing files.
#[inline]
pub fn write<P, D>(path: P, yaml: &D) -> Result<(), YamlError>
where
    P: AsRef<Path>,
    D: ?Sized + Serialize,
{
    let path = path.as_ref();

    let data = serde_yaml::to_string(&yaml).map_err(|error| YamlError::StringifyFile {
        path: path.to_path_buf(),
        error,
    })?;

    fs::write(path, data)?;

    Ok(())
}

// This function is used for consumer facing files, like configs.
#[cfg(feature = "editor-config")]
#[inline]
pub fn write_with_config<P, D>(path: P, yaml: &D) -> Result<(), YamlError>
where
    P: AsRef<Path>,
    D: ?Sized + Serialize,
{
    let path = path.as_ref();
    let editor_config = fs::get_editor_config_props(path);

    let mut data = serde_yaml::to_string(&yaml)
        .map_err(|error| YamlError::StringifyFile {
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
                    .lock()
                    .unwrap()
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

    data += &editor_config.eof;

    fs::write(path, data)?;

    Ok(())
}
