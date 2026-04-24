use std::ffi::OsStr;
use std::fmt::Debug;
use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tracing::{instrument, trace};

#[cfg(feature = "editor-config")]
pub use crate::fs_editor::*;
pub use crate::fs_error::FsError;
#[cfg(feature = "fs-lock")]
pub use crate::fs_lock::*;

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
    let mut dirs = vec![];

    trace!(
        from = ?from_root,
        to = ?to_root,
        "Copying directory"
    );

    for entry in read_dir(from_root)? {
        if let Ok(file_type) = entry.file_type() {
            let path = entry.path();
            let rel_path = path.strip_prefix(from_root).unwrap();

            if file_type.is_file() {
                copy_file(&path, to_root.join(rel_path))?;
            } else if file_type.is_dir() {
                dirs.push(rel_path.to_path_buf());
            }
        }
    }

    for dir in dirs {
        copy_dir_all(from_root.join(&dir), to_root.join(dir))?;
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

    if path.as_os_str().is_empty() {
        return Ok(());
    }

    if !path.exists() {
        trace!(dir = ?path, "Creating directory");

        fs::create_dir_all(path).map_err(|error| FsError::Create {
            path: path.to_path_buf(),
            error: Box::new(error),
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
    let dir = start_dir.as_ref();
    let name = name.as_ref();
    let findable = dir.join(name);

    trace!(
        file = name.to_str(),
        dir = ?dir,
        "Traversing upwards to find a file/root"
    );

    if findable.exists() {
        return Some(findable);
    }

    if dir == end_dir.as_ref() {
        return None;
    }

    match dir.parent() {
        Some(parent_dir) => find_upwards_until(name, parent_dir, end_dir),
        None => None,
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

/// Read direct contents for the provided directory path. If the directory
/// does not exist, an empty vector is returned.
#[inline]
#[instrument]
pub fn read_dir<T: AsRef<Path> + Debug>(path: T) -> Result<Vec<fs::DirEntry>, FsError> {
    let path = path.as_ref();
    let mut results = vec![];

    if !path.exists() {
        return Ok(results);
    }

    trace!(dir = ?path, "Reading directory");

    let entries = fs::read_dir(path).map_err(|error| FsError::Read {
        path: path.to_path_buf(),
        error: Box::new(error),
    })?;

    for entry in entries {
        match entry {
            Ok(dir) => {
                results.push(dir);
            }
            Err(error) => {
                return Err(FsError::Read {
                    path: path.to_path_buf(),
                    error: Box::new(error),
                });
            }
        }
    }

    Ok(results)
}

/// Read all contents recursively for the provided directory path.
#[inline]
#[instrument]
pub fn read_dir_all<T: AsRef<Path> + Debug>(path: T) -> Result<Vec<fs::DirEntry>, FsError> {
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

/// Remove a file or directory (recursively) at the provided path.
/// If the path does not exist, this is a no-op.
#[inline]
pub fn remove<T: AsRef<Path> + Debug>(path: T) -> Result<(), FsError> {
    let path = path.as_ref();

    if path.exists() {
        if path.is_symlink() {
            remove_link(path)?;
        } else if path.is_file() {
            remove_file(path)?;
        } else if path.is_dir() {
            remove_dir_all(path)?;
        }
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
        trace!(file = ?path, "Removing symlink");

        fs::remove_file(path).map_err(|error| FsError::Remove {
            path: path.to_path_buf(),
            error: Box::new(error),
        })?;
    }

    Ok(())
}

/// Remove a file at the provided path. If the file does not exist, this is a no-op.
#[inline]
#[instrument]
pub fn remove_file<T: AsRef<Path> + Debug>(path: T) -> Result<(), FsError> {
    let path = path.as_ref();

    if path.exists() {
        trace!(file = ?path, "Removing file");

        fs::remove_file(path).map_err(|error| FsError::Remove {
            path: path.to_path_buf(),
            error: Box::new(error),
        })?;
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

    if path.exists()
        && let Some((size, _)) = stale(path, true, duration, SystemTime::now())?
    {
        trace!(file = ?path, size, "Removing stale file");

        fs::remove_file(path).map_err(|error| FsError::Remove {
            path: path.to_path_buf(),
            error: Box::new(error),
        })?;

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

    if path.exists() {
        trace!(dir = ?path, "Removing directory");

        fs::remove_dir_all(path).map_err(|error| FsError::Remove {
            path: path.to_path_buf(),
            error: Box::new(error),
        })?;
    }

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

    if base_dir.exists() {
        trace!(dir = ?base_dir, exceptions = ?exceptions, "Removing directory with exceptions");

        fn traverse(
            base_dir: &Path,
            traverse_dir: &Path,
            exclude: &[PathBuf],
        ) -> Result<(), FsError> {
            for entry in read_dir(traverse_dir)? {
                let abs_path = entry.path();
                let rel_path = abs_path.strip_prefix(base_dir).unwrap_or(&abs_path);
                let is_excluded = exclude
                    .iter()
                    .any(|ex| rel_path == ex || ex.starts_with(rel_path));

                // Is excluded, but the relative path may be a directory,
                // so we need to continue traversing
                if is_excluded {
                    if abs_path.is_dir() {
                        traverse(base_dir, &abs_path, exclude)?;
                    }
                } else {
                    remove(abs_path)?;
                }
            }

            Ok(())
        }

        traverse(base_dir, base_dir, &exceptions)?;
    }

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

    for entry in read_dir_all(dir)? {
        if entry.file_type().is_ok_and(|file_type| file_type.is_file())
            && let Ok(bytes) = remove_file_if_stale(entry.path(), duration)
            && bytes > 0
        {
            files_deleted += 1;
            bytes_saved += bytes;
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
