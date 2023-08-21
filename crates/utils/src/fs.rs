use miette::Diagnostic;
use starbase_styles::{Style, Stylize};
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use thiserror::Error;
use tracing::trace;

#[derive(Error, Diagnostic, Debug)]
pub enum FsError {
    #[diagnostic(code(fs::copy), help("Does the source file exist?"))]
    #[error("Failed to copy {} to {}.", .from.style(Style::Path), .to.style(Style::Path))]
    Copy {
        from: PathBuf,
        to: PathBuf,
        #[source]
        error: std::io::Error,
    },

    #[diagnostic(code(fs::create))]
    #[error("Failed to create {}.", .path.style(Style::Path))]
    Create {
        path: PathBuf,
        #[source]
        error: std::io::Error,
    },

    #[diagnostic(code(fs::lock))]
    #[error("Failed to lock {}.", .path.style(Style::Path))]
    Lock {
        path: PathBuf,
        #[source]
        error: std::io::Error,
    },

    #[diagnostic(code(fs::perms))]
    #[error("Failed to update permissions for {}.", .path.style(Style::Path))]
    Perms {
        path: PathBuf,
        #[source]
        error: std::io::Error,
    },

    #[diagnostic(code(fs::read))]
    #[error("Failed to read path {}.", .path.style(Style::Path))]
    Read {
        path: PathBuf,
        #[source]
        error: std::io::Error,
    },

    #[diagnostic(code(fs::remove))]
    #[error("Failed to remove path {}.", .path.style(Style::Path))]
    Remove {
        path: PathBuf,
        #[source]
        error: std::io::Error,
    },

    #[diagnostic(code(fs::rename), help("Does the source file exist?"))]
    #[error("Failed to rename {} to {}.", .from.style(Style::Path), .to.style(Style::Path))]
    Rename {
        from: PathBuf,
        to: PathBuf,
        #[source]
        error: std::io::Error,
    },

    #[diagnostic(code(fs::unlock))]
    #[error("Failed to unlock {}.", .path.style(Style::Path))]
    Unlock {
        path: PathBuf,
        #[source]
        error: std::io::Error,
    },

    #[diagnostic(code(fs::write), help("Does the parent directory exist?"))]
    #[error("Failed to write {}.", .path.style(Style::Path))]
    Write {
        path: PathBuf,
        #[source]
        error: std::io::Error,
    },
}

/// Append a file with the provided content. If the parent directory does not exist,
/// or the file to append does not exist, they will be created.
#[inline]
pub fn append_file<T: AsRef<Path>, D: AsRef<[u8]>>(path: T, data: D) -> Result<(), FsError> {
    use std::io::Write;

    let path = path.as_ref();

    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }

    trace!(file = ?path, "Appending file");

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| FsError::Write {
            path: path.to_path_buf(),
            error,
        })?;

    file.write_all(data.as_ref())
        .map_err(|error| FsError::Write {
            path: path.to_path_buf(),
            error,
        })?;

    Ok(())
}

/// Copy a file from source to destination. If the destination directory does not exist,
/// it will be created.
#[inline]
pub fn copy_file<S: AsRef<Path>, D: AsRef<Path>>(from: S, to: D) -> Result<(), FsError> {
    let from = from.as_ref();
    let to = to.as_ref();

    if let Some(parent) = to.parent() {
        create_dir_all(parent)?;
    }

    trace!(from = ?from, to = ?to, "Copying file");

    fs::copy(from, to).map_err(|error| FsError::Copy {
        from: from.to_path_buf(),
        to: to.to_path_buf(),
        error,
    })?;

    Ok(())
}

/// Copy a directory and all of its contents from source to destination. If the destination
/// directory does not exist, it will be created.
#[inline]
pub fn copy_dir_all<T: AsRef<Path>>(from_root: T, from: T, to_root: T) -> Result<(), FsError> {
    let from_root = from_root.as_ref();
    let from = from.as_ref();
    let to_root = to_root.as_ref();
    let mut dirs = vec![];

    trace!(
        from = ?from,
        to = ?to_root,
        "Copying directory"
    );

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

/// Create a file and return a [`File`] instance. If the parent directory does not exist,
/// it will be created.
#[inline]
pub fn create_file<T: AsRef<Path>>(path: T) -> Result<File, FsError> {
    let path = path.as_ref();

    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }

    trace!(file = ?path, "Creating file");

    File::create(path).map_err(|error| FsError::Create {
        path: path.to_path_buf(),
        error,
    })
}

/// Create a directory and all parent directories if they do not exist.
/// If the directory already exists, this is a no-op.
#[inline]
pub fn create_dir_all<T: AsRef<Path>>(path: T) -> Result<(), FsError> {
    let path = path.as_ref();

    if path.as_os_str().is_empty() {
        return Ok(());
    }

    if !path.exists() {
        trace!(dir = ?path, "Creating directory");

        fs::create_dir_all(path).map_err(|error| FsError::Create {
            path: path.to_path_buf(),
            error,
        })?;
    }

    Ok(())
}

/// Return the name of a file or directory, or "unknown" if invalid UTF-8,
/// or unknown path component.
#[inline]
pub fn file_name<T: AsRef<Path>>(path: T) -> String {
    path.as_ref()
        .file_name()
        .unwrap_or_default()
        .to_str()
        .unwrap_or("<unknown>")
        .to_string()
}

/// Find a file with the provided name in the starting directory,
/// and traverse upwards until one is found. If no file is found,
/// returns [`None`].
#[inline]
pub fn find_upwards<F, P>(name: F, starting_dir: P) -> Option<PathBuf>
where
    F: AsRef<OsStr>,
    P: AsRef<Path>,
{
    let dir = starting_dir.as_ref();
    let name = name.as_ref();
    let findable = dir.join(name);

    trace!(
        file = name.to_str(),
        dir = ?dir,
        "Traversing upwards to find a file"
    );

    if findable.exists() {
        return Some(findable);
    }

    match dir.parent() {
        Some(parent_dir) => find_upwards(name, parent_dir),
        None => None,
    }
}

/// Find the root directory that contains the file with the provided name,
/// from the starting directory, and traverse upwards until one is found.
/// If no root is found, returns [`None`].
#[inline]
pub fn find_upwards_root<F, P>(name: F, starting_dir: P) -> Option<PathBuf>
where
    F: AsRef<OsStr>,
    P: AsRef<Path>,
{
    let dir = starting_dir.as_ref();
    let name = name.as_ref();
    let findable = dir.join(name);

    trace!(
        needle = ?findable,
        "Traversing upwards to find a root"
    );

    if findable.exists() {
        return Some(dir.to_path_buf());
    }

    match dir.parent() {
        Some(parent_dir) => find_upwards_root(name, parent_dir),
        None => None,
    }
}

#[cfg(feature = "editor-config")]
pub struct EditorConfigProps {
    pub eof: String,
    pub indent: String,
}

#[cfg(feature = "editor-config")]
impl EditorConfigProps {
    pub fn apply_eof(&self, data: &mut String) {
        if !self.eof.is_empty() && !data.ends_with(&self.eof) {
            data.push_str(&self.eof);
        }
    }
}

/// Load properties from the closest `.editorconfig` file.
#[cfg(feature = "editor-config")]
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

/// Lock the provided file with exclusive access and execute the operation.
#[cfg(feature = "fs-lock")]
#[inline]
pub fn lock_exclusive<T, F, V>(path: T, mut file: File, op: F) -> Result<V, FsError>
where
    T: AsRef<Path>,
    F: FnOnce(&mut File) -> Result<V, FsError>,
{
    use fs4::FileExt;

    let path = path.as_ref();

    file.lock_exclusive().map_err(|error| FsError::Lock {
        path: path.to_path_buf(),
        error,
    })?;

    let result = op(&mut file)?;

    file.unlock().map_err(|error| FsError::Unlock {
        path: path.to_path_buf(),
        error,
    })?;

    Ok(result)
}

/// Lock the provided file with shared access and execute the operation.
#[cfg(feature = "fs-lock")]
#[inline]
pub fn lock_shared<T, F, V>(path: T, mut file: File, op: F) -> Result<V, FsError>
where
    T: AsRef<Path>,
    F: FnOnce(&mut File) -> Result<V, FsError>,
{
    use fs4::FileExt;

    let path = path.as_ref();

    file.lock_shared().map_err(|error| FsError::Lock {
        path: path.to_path_buf(),
        error,
    })?;

    let result = op(&mut file)?;

    file.unlock().map_err(|error| FsError::Unlock {
        path: path.to_path_buf(),
        error,
    })?;

    Ok(result)
}

/// Return metadata for the provided path. The path must already exist.
#[inline]
pub fn metadata<T: AsRef<Path>>(path: T) -> Result<fs::Metadata, FsError> {
    let path = path.as_ref();

    trace!(file = ?path, "Reading file metadata");

    fs::metadata(path).map_err(|error| FsError::Read {
        path: path.to_path_buf(),
        error,
    })
}

/// Open a file at the provided path and return a [`File`] instance.
/// The path must already exist.
#[inline]
pub fn open_file<T: AsRef<Path>>(path: T) -> Result<File, FsError> {
    let path = path.as_ref();

    trace!(file = ?path, "Opening file");

    File::open(path).map_err(|error| FsError::Read {
        path: path.to_path_buf(),
        error,
    })
}

/// Read direct contents for the provided directory path. If the directory
/// does not exist, an empty vector is returned.
#[inline]
pub fn read_dir<T: AsRef<Path>>(path: T) -> Result<Vec<fs::DirEntry>, FsError> {
    let path = path.as_ref();

    trace!(dir = ?path, "Reading directory");

    let mut results = vec![];

    if !path.exists() {
        return Ok(results);
    }

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

/// Read all contents recursively for the provided directory path.
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

/// Read a file at the provided path into a string. The path must already exist.
#[inline]
pub fn read_file<T: AsRef<Path>>(path: T) -> Result<String, FsError> {
    let path = path.as_ref();

    trace!(file = ?path, "Reading file");

    fs::read_to_string(path).map_err(|error| FsError::Read {
        path: path.to_path_buf(),
        error,
    })
}

/// Read a file at the provided path into a bytes vector. The path must already exist.
#[inline]
pub fn read_file_bytes<T: AsRef<Path>>(path: T) -> Result<Vec<u8>, FsError> {
    let path = path.as_ref();

    trace!(file = ?path, "Reading bytes of file");

    fs::read(path).map_err(|error| FsError::Read {
        path: path.to_path_buf(),
        error,
    })
}

/// Read a file at the provided path into a string, while applying a shared lock.
/// The path must already exist.
#[cfg(feature = "fs-lock")]
#[inline]
pub fn read_file_with_lock<T: AsRef<Path>>(path: T) -> Result<String, FsError> {
    use std::io::prelude::*;

    let path = path.as_ref();

    trace!(file = ?path, "Reading file with shared lock");

    lock_shared(path, open_file(path)?, |file| {
        let mut buffer = String::new();

        file.read_to_string(&mut buffer)
            .map_err(|error| FsError::Read {
                path: path.to_path_buf(),
                error,
            })?;

        Ok(buffer)
    })
}

/// Remove a file or directory (recursively) at the provided path.
/// If the path does not exist, this is a no-op.
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

/// Remove a file at the provided path. If the file does not exist, this is a no-op.
#[inline]
pub fn remove_file<T: AsRef<Path>>(path: T) -> Result<(), FsError> {
    let path = path.as_ref();

    if path.exists() {
        trace!(file = ?path, "Removing file");

        fs::remove_file(path).map_err(|error| FsError::Remove {
            path: path.to_path_buf(),
            error,
        })?;
    }

    Ok(())
}

/// Remove a directory, and all of its contents recursively, at the provided path.
/// If the directory does not exist, this is a no-op.
#[inline]
pub fn remove_dir_all<T: AsRef<Path>>(path: T) -> Result<(), FsError> {
    let path = path.as_ref();

    if path.exists() {
        trace!(dir = ?path, "Removing directory");

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

/// Remove all contents from the provided directory path that are older than the
/// provided duration, and return a sum of bytes saved and files deleted.
/// If the directory does not exist, this is a no-op.
pub fn remove_dir_stale_contents<P: AsRef<Path>>(
    dir: P,
    duration: Duration,
) -> Result<RemoveDirContentsResult, FsError> {
    let mut files_deleted: usize = 0;
    let mut bytes_saved: u64 = 0;
    let threshold = SystemTime::now() - duration;
    let dir = dir.as_ref();

    trace!(
        dir = ?dir,
        "Removing stale contents from directory"
    );

    for entry in read_dir(dir)? {
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

/// Rename a file from source to destination. If the destination directory does not exist,
/// it will be created.
#[inline]
pub fn rename<F: AsRef<Path>, T: AsRef<Path>>(from: F, to: T) -> Result<(), FsError> {
    let from = from.as_ref();
    let to = to.as_ref();

    if let Some(parent) = to.parent() {
        create_dir_all(parent)?;
    }

    trace!(from = ?from, to = ?to, "Renaming file");

    fs::rename(from, to).map_err(|error| FsError::Rename {
        from: from.to_path_buf(),
        to: to.to_path_buf(),
        error,
    })
}

/// Update the permissions of a file at the provided path. If a mode is not provided,
/// the default of 0o755 will be used. The path must already exist.
#[cfg(unix)]
#[inline]
pub fn update_perms<T: AsRef<Path>>(path: T, mode: Option<u32>) -> Result<(), FsError> {
    use std::os::unix::fs::PermissionsExt;

    let path = path.as_ref();
    let mode = mode.unwrap_or(0o755);

    trace!(file = ?path, mode = format!("{:#02o}", mode), "Updating file permissions");

    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|error| {
        FsError::Perms {
            path: path.to_path_buf(),
            error,
        }
    })?;

    Ok(())
}

/// This is a no-op on Windows.
#[cfg(not(unix))]
#[inline]
pub fn update_perms<T: AsRef<Path>>(_path: T, _mode: Option<u32>) -> Result<(), FsError> {
    Ok(())
}

/// Write a file with the provided data to the provided path. If the parent directory
/// does not exist, it will be created.
#[inline]
pub fn write_file<T: AsRef<Path>, D: AsRef<[u8]>>(path: T, data: D) -> Result<(), FsError> {
    let path = path.as_ref();

    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }

    trace!(file = ?path, "Writing file");

    fs::write(path, data).map_err(|error| FsError::Write {
        path: path.to_path_buf(),
        error,
    })
}

/// Write a file with the provided data to the provided path, using an exclusive lock.
/// If the parent directory does not exist, it will be created.
#[cfg(feature = "fs-lock")]
#[inline]
pub fn write_file_with_lock<T: AsRef<Path>, D: AsRef<[u8]>>(
    path: T,
    data: D,
) -> Result<(), FsError> {
    use std::io::{prelude::*, SeekFrom};

    let path = path.as_ref();
    let handle_error = |error: std::io::Error| FsError::Write {
        path: path.to_path_buf(),
        error,
    };

    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }

    trace!(file = ?path, "Writing file with exclusive lock");

    // Don't use create_file() as it truncates, which will cause
    // other processes to crash if they attempt to read it while
    // the lock is active!
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .open(path)
        .map_err(handle_error)?;

    lock_exclusive(path, file, |file| {
        // Truncate then write file
        file.set_len(0).map_err(handle_error)?;
        file.seek(SeekFrom::Start(0)).map_err(handle_error)?;
        file.write(data.as_ref()).map_err(handle_error)?;

        Ok(())
    })?;

    Ok(())
}

/// Write a file with the provided data to the provided path, while taking the
/// closest `.editorconfig` into account
#[cfg(feature = "editor-config")]
#[inline]
pub fn write_file_with_config<T: AsRef<Path>, D: AsRef<[u8]>>(
    path: T,
    data: D,
) -> Result<(), FsError> {
    let path = path.as_ref();
    let editor_config = get_editor_config_props(path);

    let mut data = unsafe { String::from_utf8_unchecked(data.as_ref().to_vec()) };
    editor_config.apply_eof(&mut data);

    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }

    trace!(file = ?path, "Writing file with .editorconfig");

    fs::write(path, data).map_err(|error| FsError::Write {
        path: path.to_path_buf(),
        error,
    })
}
