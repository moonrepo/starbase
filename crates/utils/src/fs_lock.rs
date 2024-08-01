use crate::fs::{self, FsError};
use fs4::fs_std::FileExt;
use std::fmt::Debug;
use std::fs::File;
use std::path::{Path, PathBuf};
use tracing::{instrument, trace};

pub const LOCK_FILE: &str = ".lock";

pub struct DirLock {
    lock: PathBuf,
    file: File,
}

impl DirLock {
    pub fn unlock(&self) -> Result<(), FsError> {
        trace!(dir = ?self.lock.parent().unwrap(), "Unlocking directory");

        let handle_error = |error: std::io::Error| FsError::Unlock {
            path: self.lock.to_path_buf(),
            error: Box::new(error),
        };

        // On Windows this may have already been unlocked,
        // and will trigger a "already unlocked" error,
        // so account for it instead of panicing!
        #[cfg(windows)]
        if let Err(error) = self.file.unlock() {
            if error.raw_os_error().is_some_and(|os| os == 158) {
                // Ignore uncategorized: The segment is already unlocked.
            } else {
                return Err(handle_error(error));
            }
        }

        #[cfg(unix)]
        self.file.unlock().map_err(handle_error)?;

        fs::remove_file(&self.lock)
    }
}

impl Drop for DirLock {
    fn drop(&mut self) {
        self.unlock().unwrap_or_else(|error| {
            panic!(
                "Failed to remove directory lock {}: {}",
                self.lock.display(),
                error
            )
        });
    }
}

/// Return true if the directory is currently locked (via [`lock_directory`]).
pub fn is_dir_locked<T: AsRef<Path>>(path: T) -> bool {
    path.as_ref().join(LOCK_FILE).exists()
}

/// Return true if the file is currently locked (using exclusive).
/// This function operates by locking the file and checking for
/// an "is locked/contended" error, which can be brittle.
pub fn is_file_locked<T: AsRef<Path>>(path: T) -> bool {
    let Ok(file) = File::open(path) else {
        return false;
    };

    match file.try_lock_exclusive() {
        Ok(()) => {
            file.unlock().unwrap();
            false
        }
        Err(_) => true,
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
#[instrument]
pub fn lock_directory<T: AsRef<Path> + Debug>(path: T) -> Result<DirLock, FsError> {
    use std::io::prelude::*;

    let path = path.as_ref();

    fs::create_dir_all(path)?;

    if !path.is_dir() {
        return Err(FsError::RequireDir {
            path: path.to_path_buf(),
        });
    }

    trace!(dir = ?path, "Locking directory");

    // We can't rely on the existence of the `.lock` file, because if the
    // process is killed, the `DirLock` is not dropped, and the file is not removed!
    // Subsequent processes would hang thinking the directory is locked.
    //
    // Instead, we can use system-level file locking, which blocks waiting
    // for write access, and will be "unlocked" automatically by the kernel.
    //
    // Context: https://www.reddit.com/r/rust/comments/14hlx8u/comment/jpbmsh2/?utm_source=reddit&utm_medium=web2x&context=3
    let lock = path.join(LOCK_FILE);
    let mut file = fs::create_file_if_missing(&lock)?;

    trace!(
        lock = ?lock,
        "Waiting to acquire lock on directory",
    );

    // This blocks if another process has access!
    file.lock_exclusive().map_err(|error| FsError::Lock {
        path: path.to_path_buf(),
        error: Box::new(error),
    })?;

    let pid = std::process::id();

    trace!(
        lock = ?lock,
        pid,
        "Acquired lock on directory, writing PID",
    );

    // Let other processes know that we have locked it
    file.write(format!("{}", pid).as_ref())
        .map_err(|error| FsError::Write {
            path: path.to_path_buf(),
            error: Box::new(error),
        })?;

    Ok(DirLock { lock, file })
}

/// Lock the provided file with exclusive access and execute the operation.
#[inline]
#[instrument(skip(file, op))]
pub fn lock_file_exclusive<T, F, V>(path: T, mut file: File, op: F) -> Result<V, FsError>
where
    T: AsRef<Path> + Debug,
    F: FnOnce(&mut File) -> Result<V, FsError>,
{
    let path = path.as_ref();

    trace!(file = ?path, "Locking file exclusively");

    file.lock_exclusive().map_err(|error| FsError::Lock {
        path: path.to_path_buf(),
        error: Box::new(error),
    })?;

    let result = op(&mut file)?;

    file.unlock().map_err(|error| FsError::Unlock {
        path: path.to_path_buf(),
        error: Box::new(error),
    })?;

    trace!(file = ?path, "Unlocking file exclusively");

    Ok(result)
}

/// Lock the provided file with shared access and execute the operation.
#[inline]
#[instrument(skip(file, op))]
pub fn lock_file_shared<T, F, V>(path: T, mut file: File, op: F) -> Result<V, FsError>
where
    T: AsRef<Path> + Debug,
    F: FnOnce(&mut File) -> Result<V, FsError>,
{
    let path = path.as_ref();

    trace!(file = ?path, "Locking file");

    file.lock_shared().map_err(|error| FsError::Lock {
        path: path.to_path_buf(),
        error: Box::new(error),
    })?;

    let result = op(&mut file)?;

    file.unlock().map_err(|error| FsError::Unlock {
        path: path.to_path_buf(),
        error: Box::new(error),
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
                error: Box::new(error),
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
        error: Box::new(error),
    };

    // Don't use create_file() as it truncates, which will cause
    // other processes to crash if they attempt to read it while
    // the lock is active!
    lock_file_exclusive(path, fs::create_file_if_missing(path)?, |file| {
        trace!(file = ?path, "Writing file");

        // Truncate then write file
        file.set_len(0).map_err(handle_error)?;
        file.seek(SeekFrom::Start(0)).map_err(handle_error)?;
        file.write(data.as_ref()).map_err(handle_error)?;

        Ok(())
    })?;

    Ok(())
}
