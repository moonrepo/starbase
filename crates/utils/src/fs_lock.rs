use crate::fs::{self, FsError};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use tracing::trace;

pub struct DirLock {
    lock: PathBuf,
}

impl DirLock {
    pub fn unlock(&self) -> Result<(), FsError> {
        trace!(dir = ?self.lock.parent().unwrap(), "Unlocking directory");

        fs::remove_file(&self.lock)
    }
}

impl Drop for DirLock {
    fn drop(&mut self) {
        self.unlock()
            .unwrap_or_else(|_| panic!("Failed to remove directory lock {}", self.lock.display()));
    }
}

/// Lock a directory so that other processes cannot interact with it.
/// The locking mechanism works by creating a `.lock` file in the directory,
/// with the current process ID (PID) as content. If another process attempts
/// to lock the directory and the `.lock` file currently exists, it will
/// block waiting for it to be unlocked.
///
/// This function returns a `DirLock` instance that will automatically unlock
/// when being dropped.
#[inline]
pub fn lock_directory<T: AsRef<Path>>(path: T) -> Result<DirLock, FsError> {
    let path = path.as_ref();

    fs::create_dir_all(path)?;

    if !path.is_dir() {
        return Err(FsError::RequireDir {
            path: path.to_path_buf(),
        });
    }

    let lock = path.join(".lock");
    let pid = std::process::id();

    trace!(dir = ?path, pid, "Locking directory");

    loop {
        if lock.exists() {
            let lock_pid = fs::read_file_with_lock(&lock)?.parse::<u32>().ok();

            if lock_pid.is_some_and(|lid| lid == pid) {
                break;
            }

            trace!(
                lock = ?lock,
                lock_pid,
                "Lock already exists on directory, waiting 250ms for it to unlock",
            );

            thread::sleep(Duration::from_millis(250));
        } else {
            break;
        }
    }

    fs::write_file_with_lock(&lock, format!("{}", pid))?;

    Ok(DirLock { lock })
}

/// Lock the provided file with exclusive access and execute the operation.
#[inline]
pub fn lock_file_exclusive<T, F, V>(path: T, mut file: File, op: F) -> Result<V, FsError>
where
    T: AsRef<Path>,
    F: FnOnce(&mut File) -> Result<V, FsError>,
{
    use fs4::FileExt;

    let path = path.as_ref();

    trace!(file = ?path, "Locking file exclusively");

    file.lock_exclusive().map_err(|error| FsError::Lock {
        path: path.to_path_buf(),
        error,
    })?;

    let result = op(&mut file)?;

    file.unlock().map_err(|error| FsError::Unlock {
        path: path.to_path_buf(),
        error,
    })?;

    trace!(file = ?path, "Unlocking file exclusively");

    Ok(result)
}

/// Lock the provided file with shared access and execute the operation.
#[inline]
pub fn lock_file_shared<T, F, V>(path: T, mut file: File, op: F) -> Result<V, FsError>
where
    T: AsRef<Path>,
    F: FnOnce(&mut File) -> Result<V, FsError>,
{
    use fs4::FileExt;

    let path = path.as_ref();

    trace!(file = ?path, "Locking file");

    file.lock_shared().map_err(|error| FsError::Lock {
        path: path.to_path_buf(),
        error,
    })?;

    let result = op(&mut file)?;

    file.unlock().map_err(|error| FsError::Unlock {
        path: path.to_path_buf(),
        error,
    })?;

    trace!(file = ?path, "Unlocking file");

    Ok(result)
}

/// Read a file at the provided path into a string, while applying a shared lock.
/// The path must already exist.
#[inline]
pub fn read_file_with_lock<T: AsRef<Path>>(path: T) -> Result<String, FsError> {
    use std::io::prelude::*;

    let path = path.as_ref();

    lock_file_shared(path, fs::open_file(path)?, |file| {
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

    // Don't use create_file() as it truncates, which will cause
    // other processes to crash if they attempt to read it while
    // the lock is active!
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .open(path)
        .map_err(handle_error)?;

    lock_file_exclusive(path, file, |file| {
        trace!(file = ?path, "Writing file");

        // Truncate then write file
        file.set_len(0).map_err(handle_error)?;
        file.seek(SeekFrom::Start(0)).map_err(handle_error)?;
        file.write(data.as_ref()).map_err(handle_error)?;

        Ok(())
    })?;

    Ok(())
}
