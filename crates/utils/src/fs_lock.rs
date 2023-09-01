use crate::fs::{self, FsError};
use std::fs::{File, OpenOptions};
use std::path::Path;
use tracing::trace;

/// Lock the provided file with exclusive access and execute the operation.
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

/// Read a file at the provided path into a string, while applying a shared lock.
/// The path must already exist.
#[inline]
pub fn read_file_with_lock<T: AsRef<Path>>(path: T) -> Result<String, FsError> {
    use std::io::prelude::*;

    let path = path.as_ref();

    trace!(file = ?path, "Reading file with shared lock");

    lock_shared(path, fs::open_file(path)?, |file| {
        let mut buffer = String::new();

        file.read_to_string(&mut buffer)
            .map_err(|error| FsError::Read {
                path: path.to_path_buf(),
                error,
            })?;

        Ok(buffer)
    })
}

/// Write a file with the provided data to the provided path, using an exclusive lock.
/// If the parent directory does not exist, it will be created.
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
        fs::create_dir_all(parent)?;
    }

    trace!(file = ?path, "Writing file with exclusive lock");

    // Don't use create_file() as it truncates, which will cause
    // other processes to crash if they attempt to read it while
    // the lock is active!
    let file = OpenOptions::new()
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
