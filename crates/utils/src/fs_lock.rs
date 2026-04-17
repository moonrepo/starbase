#![allow(unstable_name_collisions)]

use crate::fs::{self, FsError};
use std::fmt::Debug;
use std::fs::File;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use tracing::{instrument, trace};

/// Name of the lock file used for directory locking.
pub const LOCK_FILE: &str = ".lock";

/// Instance representing a file lock (within a directory).
pub struct FileLock {
    pub file: File,
    pub path: PathBuf,

    remove: bool,
    unlocked: bool,
}

impl FileLock {
    pub fn new(path: PathBuf) -> Result<Self, FsError> {
        let file: File;

        #[cfg(unix)]
        {
            file = fs::create_file_if_missing(&path)?;
        }

        // Attempt to create/access the file in a loop
        // because this can error with "permission denied"
        // when another process has exclusive access
        #[cfg(windows)]
        {
            use std::thread::sleep;
            use std::time::Duration;

            let mut elapsed = 0;

            loop {
                match fs::create_file_if_missing(&path) {
                    Ok(inner) => {
                        file = inner;
                        break;
                    }
                    Err(error) => {
                        if let FsError::Create {
                            error: io_error, ..
                        } = &error
                        {
                            // Access denied
                            if io_error.raw_os_error().is_some_and(|code| code == 5) {
                                sleep(Duration::from_millis(100));
                                elapsed += 100;

                                // Abort after 60 seconds
                                if elapsed <= 60000 {
                                    continue;
                                }
                            }
                        }

                        return Err(error);
                    }
                }
            }
        }

        Self::with_file(path, file)
    }

    pub async fn new_async(path: PathBuf) -> Result<Self, FsError> {
        let file: File;

        #[cfg(unix)]
        {
            file = fs::create_file_if_missing(&path)?;
        }

        // Attempt to create/access the file in a loop
        // because this can error with "permission denied"
        // when another process has exclusive access
        #[cfg(windows)]
        {
            use std::time::Duration;
            use tokio::time::sleep;

            let mut elapsed = 0;

            loop {
                match fs::create_file_if_missing(&path) {
                    Ok(inner) => {
                        file = inner;
                        break;
                    }
                    Err(error) => {
                        if let FsError::Create {
                            error: io_error, ..
                        } = &error
                        {
                            // Access denied
                            if io_error.raw_os_error().is_some_and(|code| code == 5) {
                                sleep(Duration::from_millis(100)).await;
                                elapsed += 100;

                                // Abort after 60 seconds
                                if elapsed <= 60000 {
                                    continue;
                                }
                            }
                        }

                        return Err(error);
                    }
                }
            }
        }

        Self::with_file(path, file)
    }

    pub fn with_file(path: PathBuf, file: File) -> Result<Self, FsError> {
        acquire_exclusive_lock(&path, &file)?;

        Ok(Self {
            path,
            file,
            remove: false,
            unlocked: false,
        })
    }

    pub fn remove_on_unlock(&mut self) {
        self.remove = true;
    }

    pub fn unlock(&mut self) -> Result<(), FsError> {
        if self.unlocked {
            return Ok(());
        }

        // Remove the file before unlocking but swallow the error
        // so that we don't leave this file permanently locked
        if self.remove {
            let _ = fs::remove_file(&self.path);
        }

        // On Windows this may have already been unlocked,
        // and will trigger a "already unlocked" error,
        // so account for it instead of panicing!
        #[cfg(windows)]
        if let Err(error) = release_lock(&self.path, &self.file) {
            if let FsError::Unlock {
                error: io_error, ..
            } = &error
                && io_error.raw_os_error().is_some_and(|os| os == 158)
            {
                // Ignore uncategorized: The segment is already unlocked.
            } else {
                return Err(error);
            }
        }

        #[cfg(unix)]
        release_lock(&self.path, &self.file)?;

        self.unlocked = true;

        Ok(())
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        if let Err(error) = self.unlock() {
            // Only panic if the unlock error has been thrown, because that's a
            // critical error. If the remove has failed, that's not important,
            // because the file can simply be ignored and locked again.
            if matches!(error, FsError::Unlock { .. }) {
                panic!("Failed to remove lock {}: {}", self.path.display(), error);
            }
        }
    }
}

/// Instance representing a directory lock.
pub type DirLock = FileLock;

/// Return true if the directory is currently locked (via [`lock_directory`]).
/// Stale `.lock` files are ignored.
pub fn is_dir_locked<T: AsRef<Path>>(path: T) -> bool {
    let lock = path.as_ref().join(LOCK_FILE);

    if lock.exists() {
        is_file_locked(lock)
    } else {
        false
    }
}

/// Return true if the file is currently locked with an exclusive lock.
/// This uses a shared-lock probe and only reports true on actual contention.
pub fn is_file_locked<T: AsRef<Path>>(path: T) -> bool {
    let Ok(file) = File::open(path) else {
        return false;
    };

    match file.try_lock_shared() {
        Ok(()) => {
            let _ = file.unlock();
            false
        }
        Err(error) => matches!(error, std::fs::TryLockError::WouldBlock),
    }
}

/// Lock a directory so that other processes cannot interact with it.
/// The locking mechanism works by creating a `.lock` file in the directory,
/// with the current process ID (PID) as content. If another process attempts
/// to lock the directory and the `.lock` file currently exists, it will
/// block waiting for it to be unlocked.
///
/// This function returns a `DirLock` guard that will automatically unlock
/// when being dropped.
#[inline]
#[instrument]
pub fn lock_directory<T: AsRef<Path> + Debug>(path: T) -> Result<DirLock, FsError> {
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
    let mut lock = DirLock::new(path.join(LOCK_FILE))?;
    lock.remove_on_unlock();

    let pid = std::process::id();

    // Let other processes know that we have locked it
    fs::truncate_file_handle(&lock.path, &mut lock.file)?;

    lock.file
        .write_all(format!("{pid}").as_bytes())
        .map_err(|error| FsError::Write {
            path: lock.path.clone(),
            error: Box::new(error),
        })?;

    Ok(lock)
}

/// Lock the provided file with exclusive access and write the current process ID
/// as content. If another process attempts to lock the file, it will
/// block waiting for it to be unlocked.
///
/// This function returns a `FileLock` guard that will automatically unlock
/// when being dropped.
#[inline]
#[instrument]
pub fn lock_file<T: AsRef<Path> + Debug>(path: T) -> Result<FileLock, FsError> {
    let path = path.as_ref();

    if path.is_dir() {
        return Err(FsError::RequireFile {
            path: path.to_path_buf(),
        });
    }

    trace!(file = ?path, "Locking file");

    FileLock::new(path.to_path_buf())
}

/// Lock the provided file with exclusive access and execute the operation.
#[inline]
#[instrument(skip(file, op))]
pub fn run_with_exclusive_lock<T, F, V>(path: T, mut file: File, op: F) -> Result<V, FsError>
where
    T: AsRef<Path> + Debug,
    F: FnOnce(&mut File) -> Result<V, FsError>,
{
    let path = path.as_ref();

    acquire_exclusive_lock(path, &file)?;

    let result = op(&mut file)?;

    release_lock(path, &file)?;

    Ok(result)
}

/// Lock the provided file with shared access and execute the operation.
#[inline]
#[instrument(skip(file, op))]
pub fn run_with_shared_lock<T, F, V>(path: T, mut file: File, op: F) -> Result<V, FsError>
where
    T: AsRef<Path> + Debug,
    F: FnOnce(&mut File) -> Result<V, FsError>,
{
    let path = path.as_ref();

    acquire_shared_lock(path, &file)?;

    let result = op(&mut file)?;

    release_lock(path, &file)?;

    Ok(result)
}

/// Read a file at the provided path into a string, while applying a shared lock.
/// The path must already exist.
#[inline]
pub fn read_file_with_lock<T: AsRef<Path>>(path: T) -> Result<String, FsError> {
    let path = path.as_ref();

    run_with_shared_lock(path, fs::open_file(path)?, |file| {
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
    let path = path.as_ref();

    // Don't use create_file() as it truncates, which will cause
    // other processes to crash if they attempt to read it while
    // the lock is active!
    run_with_exclusive_lock(path, fs::create_file_if_missing(path)?, |file| {
        trace!(file = ?path, "Writing file");

        // Truncate then write file
        fs::truncate_file_handle(path, file)?;

        file.write_all(data.as_ref())
            .map_err(|error: std::io::Error| FsError::Write {
                path: path.to_path_buf(),
                error: Box::new(error),
            })?;

        Ok(())
    })?;

    Ok(())
}

/// Acquire an exclusive lock on the provided file, blocking until it can be acquired.
#[inline]
pub fn acquire_exclusive_lock<T: AsRef<Path> + Debug>(path: T, file: &File) -> Result<(), FsError> {
    let path = path.as_ref();

    trace!(
        file = ?path,
        "Waiting to acquire exclusive lock",
    );

    file.lock().map_err(|error| FsError::Lock {
        path: path.to_path_buf(),
        error: Box::new(error),
    })?;

    trace!(file = ?path, "Acquired exclusive lock");

    Ok(())
}

/// Acquire a shared lock on the provided file, blocking until it can be acquired.
#[inline]
pub fn acquire_shared_lock<T: AsRef<Path> + Debug>(path: T, file: &File) -> Result<(), FsError> {
    let path = path.as_ref();

    trace!(
        file = ?path,
        "Waiting to acquire shared lock",
    );

    file.lock_shared().map_err(|error| FsError::Lock {
        path: path.to_path_buf(),
        error: Box::new(error),
    })?;

    trace!(file = ?path, "Acquired shared lock");

    Ok(())
}

/// Release a lock on the provided file. This does not verify that the file is currently locked,
/// and will not error if it is not.
#[inline]
pub fn release_lock<T: AsRef<Path> + Debug>(path: T, file: &File) -> Result<(), FsError> {
    let path = path.as_ref();

    trace!(file = ?path, "Unlocking file");

    file.unlock().map_err(|error| FsError::Unlock {
        path: path.to_path_buf(),
        error: Box::new(error),
    })?;

    Ok(())
}
