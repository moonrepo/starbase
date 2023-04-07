use miette::Diagnostic;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use thiserror::Error;

#[derive(Error, Diagnostic, Debug)]
pub enum FsError {
    #[diagnostic(code(fs::copy), help("Does the source file exist?"))]
    #[error("Failed to copy <path>{from}</path> to <path>{to}</path>: {error}")]
    Copy {
        from: PathBuf,
        to: PathBuf,
        #[source]
        error: std::io::Error,
    },

    #[diagnostic(code(fs::create))]
    #[error("Failed to create <path>{path}</path>: {error}")]
    Create {
        path: PathBuf,
        #[source]
        error: std::io::Error,
    },

    #[diagnostic(code(fs::read))]
    #[error("Failed to read path <path>{path}</path>: {error}")]
    Read {
        path: PathBuf,
        #[source]
        error: std::io::Error,
    },

    #[diagnostic(code(fs::remove))]
    #[error("Failed to remove path <path>{path}</path>: {error}")]
    Remove {
        path: PathBuf,
        #[source]
        error: std::io::Error,
    },

    #[diagnostic(code(fs::rename), help("Does the source file exist?"))]
    #[error("Failed to rename <path>{from}</path> to <path>{to}</path>: {error}")]
    Rename {
        from: PathBuf,
        to: PathBuf,
        #[source]
        error: std::io::Error,
    },

    #[diagnostic(code(fs::write), help("Does the parent directory exist?"))]
    #[error("Failed to write <path>{path}</path>: {error}")]
    Write {
        path: PathBuf,
        #[source]
        error: std::io::Error,
    },
}

#[inline]
pub fn copy_file<S: AsRef<Path>, D: AsRef<Path>>(from: S, to: D) -> Result<(), FsError> {
    let from = from.as_ref();
    let to = to.as_ref();

    if let Some(parent) = to.parent() {
        create_dir_all(parent)?;
    }

    fs::copy(from, to).map_err(|error| FsError::Copy {
        from: from.to_path_buf(),
        to: to.to_path_buf(),
        error,
    })?;

    Ok(())
}

#[inline]
pub fn copy_dir_all<T: AsRef<Path>>(from_root: T, from: T, to_root: T) -> Result<(), FsError> {
    let from_root = from_root.as_ref();
    let from = from.as_ref();
    let to_root = to_root.as_ref();
    let mut dirs = vec![];

    for entry in read_dir(from)? {
        let path = entry.path();

        if path.is_file() {
            copy_file(&path, to_root.join(path.strip_prefix(from_root).unwrap()))?;
        } else if path.is_dir() {
            dirs.push(path);
        }
    }

    for dir in dirs {
        copy_dir_all(from_root, &dir, to_root)?;
    }

    Ok(())
}

#[inline]
pub fn create_dir_all<T: AsRef<Path>>(path: T) -> Result<(), FsError> {
    let path = path.as_ref();

    if !path.exists() {
        fs::create_dir_all(path).map_err(|error| FsError::Create {
            path: path.to_path_buf(),
            error,
        })?;
    }

    Ok(())
}

#[inline]
pub fn file_name<T: AsRef<Path>>(path: T) -> String {
    path.as_ref()
        .file_name()
        .unwrap_or_default()
        .to_str()
        .unwrap_or("<unknown>")
        .to_string()
}

#[inline]
pub fn find_upwards<F, P>(name: F, starting_dir: P) -> Option<PathBuf>
where
    F: AsRef<OsStr>,
    P: AsRef<Path>,
{
    let dir = starting_dir.as_ref();
    let findable = dir.join(name.as_ref());

    if findable.exists() {
        return Some(findable);
    }

    match dir.parent() {
        Some(parent_dir) => find_upwards(name, parent_dir),
        None => None,
    }
}

pub struct EditorConfigProps {
    pub eof: String,
    pub indent: String,
}

pub fn get_editor_config_props<T: AsRef<Path>>(path: T) -> EditorConfigProps {
    use ec4rs::property::*;

    let editor_config = ec4rs::properties_of(path).unwrap_or_default();
    let tab_width = editor_config
        .get::<TabWidth>()
        .unwrap_or(TabWidth::Value(4));
    let indent_size = editor_config
        .get::<IndentSize>()
        .unwrap_or(IndentSize::Value(2));
    let indent_style = editor_config
        .get::<IndentStyle>()
        .unwrap_or(IndentStyle::Spaces);
    let insert_final_newline = editor_config
        .get::<FinalNewline>()
        .unwrap_or(FinalNewline::Value(true));

    EditorConfigProps {
        eof: if matches!(insert_final_newline, FinalNewline::Value(true)) {
            "\n".into()
        } else {
            "".into()
        },
        indent: match indent_style {
            IndentStyle::Tabs => "\t".into(),
            IndentStyle::Spaces => match indent_size {
                IndentSize::UseTabWidth => match tab_width {
                    TabWidth::Value(value) => " ".repeat(value),
                },
                IndentSize::Value(value) => " ".repeat(value),
            },
        },
    }
}

#[inline]
pub fn metadata<T: AsRef<Path>>(path: T) -> Result<fs::Metadata, FsError> {
    let path = path.as_ref();

    fs::metadata(path).map_err(|error| FsError::Read {
        path: path.to_path_buf(),
        error,
    })
}

#[inline]
pub fn read_dir<T: AsRef<Path>>(path: T) -> Result<Vec<fs::DirEntry>, FsError> {
    let path = path.as_ref();

    let mut results = vec![];
    let entries = fs::read_dir(path).map_err(|error| FsError::Read {
        path: path.to_path_buf(),
        error,
    })?;

    for entry in entries {
        match entry {
            Ok(dir) => {
                results.push(dir);
            }
            Err(error) => {
                return Err(FsError::Read {
                    path: path.to_path_buf(),
                    error,
                });
            }
        }
    }

    Ok(results)
}

#[inline]
pub fn read_dir_all<T: AsRef<Path>>(path: T) -> Result<Vec<fs::DirEntry>, FsError> {
    let entries = read_dir(path)?;
    let mut results = vec![];

    for entry in entries {
        if let Ok(file_type) = entry.file_type() {
            if file_type.is_dir() {
                results.extend(read_dir_all(entry.path())?);
            } else {
                results.push(entry);
            }
        }
    }

    Ok(results)
}

#[inline]
pub fn read<T: AsRef<Path>>(path: T) -> Result<String, FsError> {
    let path = path.as_ref();

    fs::read_to_string(path).map_err(|error| FsError::Read {
        path: path.to_path_buf(),
        error,
    })
}

#[inline]
pub fn remove<T: AsRef<Path>>(path: T) -> Result<(), FsError> {
    let path = path.as_ref();

    if path.exists() {
        if path.is_file() {
            remove_file(path)?;
        } else if path.is_dir() {
            remove_dir_all(path)?;
        }
    }

    Ok(())
}

#[inline]
pub fn remove_file<T: AsRef<Path>>(path: T) -> Result<(), FsError> {
    let path = path.as_ref();

    if path.exists() {
        fs::remove_file(path).map_err(|error| FsError::Remove {
            path: path.to_path_buf(),
            error,
        })?;
    }

    Ok(())
}

#[inline]
pub fn remove_dir_all<T: AsRef<Path>>(path: T) -> Result<(), FsError> {
    let path = path.as_ref();

    if path.exists() {
        fs::remove_dir_all(path).map_err(|error| FsError::Remove {
            path: path.to_path_buf(),
            error,
        })?;
    }

    Ok(())
}

pub struct RemoveDirContentsResult {
    pub files_deleted: usize,
    pub bytes_saved: u64,
}

pub fn remove_dir_stale_contents<P: AsRef<Path>>(
    dir: P,
    duration: Duration,
) -> Result<RemoveDirContentsResult, FsError> {
    let mut files_deleted: usize = 0;
    let mut bytes_saved: u64 = 0;
    let threshold = SystemTime::now() - duration;

    for entry in read_dir(dir.as_ref())? {
        let path = entry.path();

        if path.is_file() {
            let mut bytes = 0;

            if let Ok(metadata) = entry.metadata() {
                bytes = metadata.len();

                if let Ok(filetime) = metadata.accessed().or_else(|_| metadata.created()) {
                    if filetime > threshold {
                        // Not stale yet
                        continue;
                    }
                } else {
                    // Not supported in environment
                    continue;
                }
            }

            if remove_file(path).is_ok() {
                files_deleted += 1;
                bytes_saved += bytes;
            }
        }
    }

    Ok(RemoveDirContentsResult {
        files_deleted,
        bytes_saved,
    })
}

#[inline]
pub fn rename<F: AsRef<Path>, T: AsRef<Path>>(from: F, to: T) -> Result<(), FsError> {
    let from = from.as_ref();
    let to = to.as_ref();

    if let Some(parent) = to.parent() {
        create_dir_all(parent)?;
    }

    fs::rename(from, to).map_err(|error| FsError::Rename {
        from: from.to_path_buf(),
        to: to.to_path_buf(),
        error,
    })
}

#[inline]
pub fn write<T: AsRef<Path>, D: AsRef<[u8]>>(path: T, data: D) -> Result<(), FsError> {
    let path = path.as_ref();

    fs::write(path, data).map_err(|error| FsError::Write {
        path: path.to_path_buf(),
        error,
    })
}
