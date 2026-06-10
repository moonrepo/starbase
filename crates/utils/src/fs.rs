use reflink_copy::reflink_or_copy;
use std::ffi::OsStr;
use std::fmt::Debug;
use std::fs::{self, DirEntry, File, FileType, OpenOptions};
use std::io::{ErrorKind, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tracing::{instrument, trace};

#[cfg(feature = "editor-config")]
pub use crate::fs_editor::*;
pub use crate::fs_error::FsError;
#[cfg(feature = "fs-lock")]
pub use crate::fs_lock::*;

fn read_dir_iter(path: &Path) -> Result<Option<fs::ReadDir>, FsError> {
    match fs::read_dir(path) {
        Ok(entries) => Ok(Some(entries)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(FsError::Read {
            path: path.to_path_buf(),
            error: Box::new(error),
        }),
    }
}

fn extract_entry_and_type(
    path: &Path,
    entry: std::io::Result<DirEntry>,
) -> Result<Option<(DirEntry, FileType)>, FsError> {
    let entry = entry.map_err(|error| FsError::Read {
        path: path.to_path_buf(),
        error: Box::new(error),
    })?;

    let Ok(file_type) = entry.file_type() else {
        return Ok(None);
    };

    Ok(Some((entry, file_type)))
}

/// Append a file with the provided content. If the parent directory does not exist,
/// or the file to append does not exist, they will be created.
#[inline]
#[instrument(skip(data))]
pub fn append_file<D: AsRef<[u8]>>(path: impl AsRef<Path> + Debug, data: D) -> Result<(), FsError> {
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
            error: Box::new(error),
        })?;

    file.write_all(data.as_ref())
        .map_err(|error| FsError::Write {
            path: path.to_path_buf(),
            error: Box::new(error),
        })?;

    Ok(())
}

/// Copy a file from source to destination. If the destination directory does not exist,
/// it will be created.
#[inline]
#[instrument]
pub fn copy_file<S: AsRef<Path> + Debug, D: AsRef<Path> + Debug>(
    from: S,
    to: D,
) -> Result<(), FsError> {
    let from = from.as_ref();
    let to = to.as_ref();

    if let Some(parent) = to.parent() {
        create_dir_all(parent)?;
    }

    trace!(from = ?from, to = ?to, "Copying file");

    fs::copy(from, to).map_err(|error| FsError::Copy {
        from: from.to_path_buf(),
        to: to.to_path_buf(),
        error: Box::new(error),
    })?;

    Ok(())
}

/// Copy a directory and all of its contents from source to destination. If the destination
/// directory does not exist, it will be created.
#[inline]
#[instrument]
pub fn copy_dir_all<F: AsRef<Path> + Debug, T: AsRef<Path> + Debug>(
    from_root: F,
    to_root: T,
) -> Result<(), FsError> {
    let from_root = from_root.as_ref();
    let to_root = to_root.as_ref();

    let Some(entries) = read_dir_iter(from_root)? else {
        return Ok(());
    };

    trace!(
        from = ?from_root,
        to = ?to_root,
        "Copying directory"
    );

    for entry in entries {
        let Some((entry, file_type)) = extract_entry_and_type(from_root, entry)? else {
            continue;
        };

        let from_path = entry.path();
        let to_path = to_root.join(entry.file_name());

        if file_type.is_file() {
            copy_file(from_path, to_path)?;
        } else if file_type.is_dir() {
            copy_dir_all(from_path, to_path)?;
        }
    }

    Ok(())
}

/// Create a file and return a [`File`] instance. If the parent directory does not exist,
/// it will be created.
#[inline]
#[instrument]
pub fn create_file<T: AsRef<Path> + Debug>(path: T) -> Result<File, FsError> {
    let path = path.as_ref();

    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }

    trace!(file = ?path, "Creating file");

    File::create(path).map_err(|error| FsError::Create {
        path: path.to_path_buf(),
        error: Box::new(error),
    })
}

/// Like [`create_file`] but does not truncate existing file contents,
/// and only creates if the file is missing.
#[inline]
#[instrument]
pub fn create_file_if_missing<T: AsRef<Path> + Debug>(path: T) -> Result<File, FsError> {
    let path = path.as_ref();

    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }

    trace!(file = ?path, "Creating file without truncating");

    #[allow(clippy::suspicious_open_options)]
    OpenOptions::new()
        .write(true)
        .create(true)
        .open(path)
        .map_err(|error| FsError::Create {
            path: path.to_path_buf(),
            error: Box::new(error),
        })
}

/// Create a directory and all parent directories if they do not exist.
/// If the directory already exists, this is a no-op.
#[inline]
#[instrument]
pub fn create_dir_all<T: AsRef<Path> + Debug>(path: T) -> Result<(), FsError> {
    let path = path.as_ref();

    // We must use an exists check here because on Unix platforms,
    // the `create_dir_all` acts like `mkdir -p` and will not trigger
    // the `AlreadyExists` error if the path already exists,
    // which results in the logs being spammed with "Creating directory"
    if path.as_os_str().is_empty() || path.exists() {
        return Ok(());
    }

    match fs::create_dir_all(path) {
        Ok(()) => {
            trace!(dir = ?path, "Creating directory");
        }
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
        Err(error) => {
            return Err(FsError::Create {
                path: path.to_path_buf(),
                error: Box::new(error),
            });
        }
    };

    Ok(())
}

/// Return the name of a file or directory, or "unknown" if invalid UTF-8,
/// or unknown path component.
#[inline]
pub fn file_name<T: AsRef<Path>>(path: T) -> String {
    path.as_ref()
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("<unknown>")
        .to_owned()
}

/// Find a file with the provided name in the starting directory,
/// and traverse upwards until one is found. If no file is found,
/// returns [`None`].
#[inline]
pub fn find_upwards<F, P>(name: F, start_dir: P) -> Option<PathBuf>
where
    F: AsRef<OsStr> + Debug,
    P: AsRef<Path> + Debug,
{
    find_upwards_until(name, start_dir, PathBuf::from("/"))
}

/// Find a file with the provided name in the starting directory,
/// and traverse upwards until one is found, or stop traversing
/// if we hit the ending directory. If no file is found, returns [`None`].
#[inline]
#[instrument]
pub fn find_upwards_until<F, S, E>(name: F, start_dir: S, end_dir: E) -> Option<PathBuf>
where
    F: AsRef<OsStr> + Debug,
    S: AsRef<Path> + Debug,
    E: AsRef<Path> + Debug,
{
    let name = name.as_ref();
    let mut dir = start_dir.as_ref();
    let end_dir = end_dir.as_ref();

    trace!(
        file = name.to_str(),
        dir = ?dir,
        "Traversing upwards to find a file/root"
    );

    loop {
        let findable = dir.join(name);

        if findable.exists() {
            return Some(findable);
        }

        if dir == end_dir {
            return None;
        }

        match dir.parent() {
            Some(parent_dir) => dir = parent_dir,
            None => return None,
        }
    }
}

/// Find the root directory that contains the file with the provided name,
/// from the starting directory, and traverse upwards until one is found.
/// If no root is found, returns [`None`].
#[inline]
pub fn find_upwards_root<F, P>(name: F, start_dir: P) -> Option<PathBuf>
where
    F: AsRef<OsStr> + Debug,
    P: AsRef<Path> + Debug,
{
    find_upwards_root_until(name, start_dir, PathBuf::from("/"))
}

/// Find the root directory that contains the file with the provided name,
/// from the starting directory, and traverse upwards until one is found,
/// or stop traversing if we hit the ending directory. If no root is found,
/// returns [`None`].
#[inline]
pub fn find_upwards_root_until<F, S, E>(name: F, start_dir: S, end_dir: E) -> Option<PathBuf>
where
    F: AsRef<OsStr> + Debug,
    S: AsRef<Path> + Debug,
    E: AsRef<Path> + Debug,
{
    find_upwards_until(name, start_dir, end_dir).map(|p| p.parent().unwrap().to_path_buf())
}

/// Check if the provided path is executable. On Unix, this checks if the file has any executable
/// permissions. On Windows, this checks if the file extension is `.exe`.
#[cfg(unix)]
pub fn is_executable<T: AsRef<Path>>(path: T) -> bool {
    use std::os::unix::fs::PermissionsExt;

    fs::metadata(path.as_ref())
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// Check if the provided path is executable. On Unix, this checks if the file has any executable
/// permissions. On Windows, this checks if the file extension is `.exe`.
#[cfg(windows)]
pub fn is_executable<T: AsRef<Path>>(path: T) -> bool {
    path.as_ref().extension().is_some_and(|ext| ext == "exe")
}

/// Check if the provided path is a stale file, by comparing modified, created, or accessed
/// timestamps against the current timestamp and duration. If stale, returns a boolean.
#[inline]
#[instrument]
pub fn is_stale<T: AsRef<Path> + Debug>(
    path: T,
    accessed: bool,
    duration: Duration,
) -> Result<bool, FsError> {
    stale(path, accessed, duration, SystemTime::now()).map(|res| res.is_some())
}

/// Return metadata for the provided path. The path must already exist.
#[inline]
#[instrument]
pub fn metadata<T: AsRef<Path> + Debug>(path: T) -> Result<fs::Metadata, FsError> {
    let path = path.as_ref();

    trace!(file = ?path, "Reading file metadata");

    fs::metadata(path).map_err(|error| FsError::Read {
        path: path.to_path_buf(),
        error: Box::new(error),
    })
}

/// Open a file in read-only mode at the provided path and return a [`File`] instance.
/// The path must exist or an error is returned.
#[inline]
#[instrument]
pub fn open_file<T: AsRef<Path> + Debug>(path: T) -> Result<File, FsError> {
    let path = path.as_ref();

    trace!(file = ?path, "Opening file in read-only mode");

    File::open(path).map_err(|error| FsError::Read {
        path: path.to_path_buf(),
        error: Box::new(error),
    })
}

/// Open a file in append-write mode at the provided path and return a [`File`] instance.
/// The path must exist or an error is returned.
#[inline]
#[instrument]
pub fn open_file_for_appending<T: AsRef<Path> + Debug>(path: T) -> Result<File, FsError> {
    let path = path.as_ref();

    trace!(file = ?path, "Opening file in append mode");

    let file = OpenOptions::new()
        .read(true)
        .append(true)
        .open(path)
        .map_err(|error| FsError::Write {
            path: path.to_path_buf(),
            error: Box::new(error),
        })?;

    Ok(file)
}

/// Open a file in read-write mode at the provided path and return a [`File`] instance.
/// The path must exist or an error is returned.
#[inline]
#[instrument]
pub fn open_file_for_writing<T: AsRef<Path> + Debug>(path: T) -> Result<File, FsError> {
    let path = path.as_ref();

    trace!(file = ?path, "Opening file in write mode");

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| FsError::Write {
            path: path.to_path_buf(),
            error: Box::new(error),
        })?;

    Ok(file)
}

/// Eagerly read direct contents for the provided directory path. If the directory
/// does not exist, an empty vector is returned.
#[inline]
#[instrument]
pub fn read_dir<T: AsRef<Path> + Debug>(path: T) -> Result<Vec<DirEntry>, FsError> {
    let path = path.as_ref();
    let mut results = vec![];

    let Some(entries) = read_dir_iter(path)? else {
        return Ok(results);
    };

    trace!(dir = ?path, "Reading directory");

    for entry in entries {
        results.push(entry.map_err(|error| FsError::Read {
            path: path.to_path_buf(),
            error: Box::new(error),
        })?);
    }

    Ok(results)
}

/// Eagerly read all contents recursively for the provided directory path.
#[inline]
#[instrument]
pub fn read_dir_all<T: AsRef<Path> + Debug>(path: T) -> Result<Vec<DirEntry>, FsError> {
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
#[instrument]
pub fn read_file<T: AsRef<Path> + Debug>(path: T) -> Result<String, FsError> {
    let path = path.as_ref();

    trace!(file = ?path, "Reading file");

    fs::read_to_string(path).map_err(|error| FsError::Read {
        path: path.to_path_buf(),
        error: Box::new(error),
    })
}

/// Read a file at the provided path into a bytes vector. The path must already exist.
#[inline]
#[instrument]
pub fn read_file_bytes<T: AsRef<Path> + Debug>(path: T) -> Result<Vec<u8>, FsError> {
    let path = path.as_ref();

    trace!(file = ?path, "Reading bytes of file");

    fs::read(path).map_err(|error| FsError::Read {
        path: path.to_path_buf(),
        error: Box::new(error),
    })
}

/// Reflink a file from source to destination. If the destination directory does not exist,
/// it will be created. If the reflink fails, a fallback copy will be used instead.
#[inline]
#[instrument]
pub fn reflink_file<S: AsRef<Path> + Debug, D: AsRef<Path> + Debug>(
    from: S,
    to: D,
) -> Result<(), FsError> {
    let from = from.as_ref();
    let to = to.as_ref();

    if let Some(parent) = to.parent() {
        create_dir_all(parent)?;
    }

    trace!(from = ?from, to = ?to, "Reflinking file");

    reflink_or_copy(from, to).map_err(|error| FsError::Copy {
        from: from.to_path_buf(),
        to: to.to_path_buf(),
        error: Box::new(error),
    })?;

    Ok(())
}

/// Remove a file or directory (recursively) at the provided path.
/// If the path does not exist, this is a no-op.
#[inline]
pub fn remove<T: AsRef<Path> + Debug>(path: T) -> Result<(), FsError> {
    let path = path.as_ref();

    if path.is_symlink() {
        remove_link(path)?;
    } else if path.is_file() {
        remove_file(path)?;
    } else if path.is_dir() {
        remove_dir_all(path)?;
    }

    Ok(())
}

/// Remove a symlink at the provided path. If the file does not exist, or is not a
/// symlink, this is a no-op.
#[inline]
#[instrument]
pub fn remove_link<T: AsRef<Path> + Debug>(path: T) -> Result<(), FsError> {
    let path = path.as_ref();

    // We can't use an `exists` check as it will return false if the source file
    // no longer exists, but the symlink does exist (broken link). To actually
    // remove the symlink when in a broken state, we need to read the metadata
    // and infer the state ourself.
    if let Ok(metadata) = path.symlink_metadata()
        && metadata.is_symlink()
    {
        match fs::remove_file(path) {
            Ok(()) => {
                trace!(file = ?path, "Removing symlink");
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(FsError::Remove {
                    path: path.to_path_buf(),
                    error: Box::new(error),
                });
            }
        };
    }

    Ok(())
}

/// Remove a file at the provided path. If the file does not exist, this is a no-op.
#[inline]
#[instrument]
pub fn remove_file<T: AsRef<Path> + Debug>(path: T) -> Result<(), FsError> {
    let path = path.as_ref();

    // We use an exists check to avoid removing broken symlinks.
    // Refer to the `remove_link` function for more details.
    if path.exists() {
        match fs::remove_file(path) {
            Ok(()) => {
                trace!(file = ?path, "Removing file");
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(FsError::Remove {
                    path: path.to_path_buf(),
                    error: Box::new(error),
                });
            }
        };
    }

    Ok(())
}

/// Remove a file at the provided path if it's older than the provided duration.
/// If the file does not exist, or is younger than the duration, this is a no-op.
#[inline]
#[instrument]
pub fn remove_file_if_stale<T: AsRef<Path> + Debug>(
    path: T,
    duration: Duration,
) -> Result<u64, FsError> {
    let path = path.as_ref();

    if let Some((size, _)) = stale(path, true, duration, SystemTime::now())? {
        remove_file(path)?;

        return Ok(size);
    }

    Ok(0)
}

/// Remove a directory, and all of its contents recursively, at the provided path.
/// If the directory does not exist, this is a no-op.
#[inline]
#[instrument]
pub fn remove_dir_all<T: AsRef<Path> + Debug>(path: T) -> Result<(), FsError> {
    let path = path.as_ref();

    match fs::remove_dir_all(path) {
        Ok(()) => {
            trace!(dir = ?path, "Removing directory");
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => {
            return Err(FsError::Remove {
                path: path.to_path_buf(),
                error: Box::new(error),
            });
        }
    };

    Ok(())
}

/// Remove a directory, and all of its contents recursively, except for the provided list
/// of relative paths. If the directory does not exist, this is a no-op.
#[inline]
#[instrument]
pub fn remove_dir_all_except<T: AsRef<Path> + Debug>(
    path: T,
    exceptions: Vec<PathBuf>,
) -> Result<(), FsError> {
    let base_dir = path.as_ref();

    trace!(dir = ?base_dir, exceptions = ?exceptions, "Removing directory with exceptions");

    fn traverse(base_dir: &Path, traverse_dir: &Path, exclude: &[PathBuf]) -> Result<(), FsError> {
        let Some(entries) = read_dir_iter(traverse_dir)? else {
            return Ok(());
        };

        for entry in entries {
            let Some((entry, file_type)) = extract_entry_and_type(traverse_dir, entry)? else {
                continue;
            };

            let abs_path = entry.path();
            let rel_path = abs_path.strip_prefix(base_dir).unwrap_or(&abs_path);
            let is_excluded = exclude
                .iter()
                .any(|ex| rel_path == ex || rel_path.starts_with(ex));
            let contains_excluded_path = exclude.iter().any(|ex| ex.starts_with(rel_path));

            if is_excluded {
                continue;
            } else if contains_excluded_path && file_type.is_dir() {
                traverse(base_dir, &abs_path, exclude)?;
            } else if file_type.is_dir() {
                remove_dir_all(abs_path)?;
            } else if file_type.is_symlink() {
                remove_link(abs_path)?;
            } else if file_type.is_file() {
                remove_file(abs_path)?;
            }
        }

        Ok(())
    }

    traverse(base_dir, base_dir, &exceptions)?;

    Ok(())
}

/// Result of removing stale contents from a directory, including the number of files deleted,
/// and the total bytes saved.
pub struct RemoveDirContentsResult {
    pub files_deleted: usize,
    pub bytes_saved: u64,
}

/// Remove all contents from the provided directory path that are older than the
/// provided duration, and return a sum of bytes saved and files deleted.
/// If the directory does not exist, this is a no-op.
#[instrument]
pub fn remove_dir_stale_contents<P: AsRef<Path> + Debug>(
    dir: P,
    duration: Duration,
) -> Result<RemoveDirContentsResult, FsError> {
    let mut files_deleted: usize = 0;
    let mut bytes_saved: u64 = 0;
    let dir = dir.as_ref();

    trace!(
        dir = ?dir,
        "Removing stale contents from directory"
    );

    fn traverse(
        dir: &Path,
        duration: Duration,
        files_deleted: &mut usize,
        bytes_saved: &mut u64,
    ) -> Result<(), FsError> {
        let Some(entries) = read_dir_iter(dir)? else {
            return Ok(());
        };

        for entry in entries {
            let Some((entry, file_type)) = extract_entry_and_type(dir, entry)? else {
                continue;
            };

            let path = entry.path();

            if file_type.is_dir() {
                traverse(&path, duration, files_deleted, bytes_saved)?;
            } else if file_type.is_file()
                && let Ok(size) = remove_file_if_stale(path, duration)
            {
                *files_deleted += 1;
                *bytes_saved += size;
            }
        }

        Ok(())
    }

    traverse(dir, duration, &mut files_deleted, &mut bytes_saved)?;

    Ok(RemoveDirContentsResult {
        files_deleted,
        bytes_saved,
    })
}

/// Rename a file from source to destination. If the destination directory does not exist,
/// it will be created.
#[inline]
#[instrument]
pub fn rename<F: AsRef<Path> + Debug, T: AsRef<Path> + Debug>(
    from: F,
    to: T,
) -> Result<(), FsError> {
    let from = from.as_ref();
    let to = to.as_ref();

    if let Some(parent) = to.parent() {
        create_dir_all(parent)?;
    }

    trace!(from = ?from, to = ?to, "Renaming file");

    fs::rename(from, to).map_err(|error| FsError::Rename {
        from: from.to_path_buf(),
        to: to.to_path_buf(),
        error: Box::new(error),
    })
}

/// Check if the provided path is a stale file, by comparing modified, created, or accessed
/// timestamps against the current timestamp and duration. If stale, return the file size
/// and timestamp, otherwise return `None`.
#[inline]
#[instrument]
pub fn stale<T: AsRef<Path> + Debug>(
    path: T,
    accessed: bool,
    duration: Duration,
    current_time: SystemTime,
) -> Result<Option<(u64, SystemTime)>, FsError> {
    let path = path.as_ref();

    // Avoid bubbling up result errors and just mark as stale
    if let Ok(meta) = metadata(path) {
        let mut time = meta.modified().or_else(|_| meta.created());

        if accessed && let Ok(accessed_time) = meta.accessed() {
            time = Ok(accessed_time);
        }

        if let Ok(check_time) = time
            && check_time < (current_time - duration)
        {
            return Ok(Some((meta.len(), check_time)));
        }
    }

    Ok(None)
}

/// Truncate a file at the provided handle to zero length, and reset
/// the cursor to the start of the file.
#[inline]
#[instrument]
pub fn truncate_file_handle<T: AsRef<Path> + Debug>(
    path: T,
    file: &mut File,
) -> Result<(), FsError> {
    let path = path.as_ref();

    file.set_len(0).map_err(|error| FsError::Write {
        path: path.to_owned(),
        error: Box::new(error),
    })?;

    file.seek(SeekFrom::Start(0))
        .map_err(|error| FsError::Write {
            path: path.to_owned(),
            error: Box::new(error),
        })?;

    Ok(())
}

/// Update the permissions of a file at the provided path. If a mode is not provided,
/// the default of 0o755 will be used. The path must already exist.
#[cfg(unix)]
#[inline]
#[instrument]
pub fn update_perms<T: AsRef<Path> + Debug>(path: T, mode: Option<u32>) -> Result<(), FsError> {
    use std::os::unix::fs::PermissionsExt;

    let path = path.as_ref();
    let mode = mode.unwrap_or(0o755);

    trace!(file = ?path, mode = format!("{:#02o}", mode), "Updating file permissions");

    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|error| {
        FsError::Perms {
            path: path.to_path_buf(),
            error: Box::new(error),
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
#[instrument(skip(data))]
pub fn write_file<D: AsRef<[u8]>>(path: impl AsRef<Path> + Debug, data: D) -> Result<(), FsError> {
    let path = path.as_ref();

    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }

    trace!(file = ?path, "Writing file");

    fs::write(path, data).map_err(|error| FsError::Write {
        path: path.to_path_buf(),
        error: Box::new(error),
    })
}
