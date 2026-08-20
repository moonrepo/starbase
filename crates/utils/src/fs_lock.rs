//! File and directory locking built on advisory system-level locks
//! (`flock` on Unix, `LockFileEx` on Windows).
//!
//! System locks bind to the *open file* (inode on Unix, file object on
//! Windows) while processes rendezvous by *path*, so any protocol that
//! deletes lock files must bridge that gap. We follow the canonical ordering:
//!
//! - Acquire: open (never truncate) -> lock -> validate the path still
//!   resolves to the locked file (retry if not) -> only then write content.
//! - Release: delete (while still holding the lock) -> unlock -> close.
//!
//! Deleting before unlocking guarantees only ever the current holder deletes
//! (a single-deleter invariant), and validating after locking is how waiters
//! detect that a holder deleted the file out from under them. Each half
//! requires the other; see [`is_lock_current`] for the platform specifics.

use crate::fs::{self, FsError};
use std::fmt::Debug;
use std::fs::{File, OpenOptions};
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use tracing::{error, instrument, trace};

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
        let file = acquire_validated(&path, true, create_lock_file)?;

        Ok(Self {
            path,
            file,
            remove: false,
            unlocked: false,
        })
    }

    pub async fn new_async(path: PathBuf) -> Result<Self, FsError> {
        let task_path = path.clone();

        run_blocking(path, move || Self::new(task_path)).await
    }

    /// Lock the provided pre-opened file. If the file was removed or replaced
    /// before the lock was acquired, the stale handle is discarded and the
    /// lock is re-acquired against the current file at the path (using a
    /// standard read/write handle, not the provided one).
    pub fn with_file(path: PathBuf, file: File) -> Result<Self, FsError> {
        let file = lock_handle_validated(&path, file)?;

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

        // Remove the file while we still hold the lock, so that we can only
        // ever be deleting our own file (no other process holds the lock at
        // this instant), and swallow the error so that we don't leave this
        // file permanently locked. Deleting an in-use lock file orphans it for
        // any process still waiting on it — those waiters detect this via
        // `is_lock_current` and re-acquire against the recreated file.
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
            // Only surface unlock errors, as those are critical. A failed remove
            // isn't important, since the file can be ignored and locked again.
            if matches!(error, FsError::Unlock { .. }) {
                // Panicking while another panic is already unwinding aborts the
                // entire process, so downgrade to an error log in that case.
                if std::thread::panicking() {
                    error!("Failed to unlock {}: {error}", self.path.display());
                } else {
                    panic!("Failed to unlock {}: {error}", self.path.display());
                }
            }
        }
    }
}

/// Create or open a file that is about to be locked, without truncating
/// existing content (which we don't own until the lock is acquired). The file
/// is opened with both read and write access — the metadata queries used for
/// lock validation require read access that write-only handles may lack on
/// Windows. If the parent directory does not exist, it will be created.
fn open_lockable_file(path: &Path) -> Result<File, FsError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|error| FsError::Create {
            path: path.to_path_buf(),
            error: Box::new(error),
        })
}

/// Create or open the lock file at the provided path, without truncating it.
///
/// On Windows the open may fail with "access denied" while another process
/// holds the file exclusively, or while a removed lock file is still pending
/// deletion, so we retry in a loop for up to 60 seconds.
fn create_lock_file(path: &Path) -> Result<File, FsError> {
    let file: File;

    #[cfg(not(windows))]
    {
        file = open_lockable_file(path)?;
    }

    #[cfg(windows)]
    {
        use std::thread::sleep;
        use std::time::Duration;

        let mut elapsed = 0;

        loop {
            match open_lockable_file(path) {
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

    Ok(file)
}

/// Run a lock acquisition on a blocking thread instead of stalling the async
/// runtime, as acquiring a lock blocks the current thread, potentially for a
/// long time.
async fn run_blocking<V: Send + 'static>(
    path: PathBuf,
    task: impl FnOnce() -> Result<V, FsError> + Send + 'static,
) -> Result<V, FsError> {
    match tokio::task::spawn_blocking(task).await {
        Ok(result) => result,
        Err(error) => {
            if error.is_panic() {
                std::panic::resume_unwind(error.into_panic());
            }

            // Cancelled, which only happens when the runtime is shutting down
            Err(FsError::Lock {
                path,
                error: Box::new(std::io::Error::other(
                    "async runtime shut down while acquiring the lock",
                )),
            })
        }
    }
}

/// Exclusively lock the provided pre-opened file handle, validating it after
/// acquisition. If the file was removed or replaced at the path before the
/// lock was acquired, the stale handle is discarded (closing it releases the
/// lock) and a new validated lock is acquired against the current file at the
/// path, which is returned in place of the provided handle.
pub(crate) fn lock_handle_validated(path: &Path, file: File) -> Result<File, FsError> {
    acquire_exclusive_lock(path, &file)?;

    if is_lock_current(path, &file)? {
        return Ok(file);
    }

    // Close releases the lock regardless, so a failed release is harmless
    let _ = release_lock(path, &file);

    drop(file);

    trace!(file = ?path, "Lock file was replaced while locking, re-acquiring");

    acquire_validated(path, true, create_lock_file)
}

/// Async variant of [`lock_handle_validated`] that acquires on a blocking
/// thread instead of stalling the async runtime.
#[cfg_attr(not(feature = "net"), allow(dead_code))]
pub(crate) async fn lock_handle_validated_async(
    path: PathBuf,
    file: File,
) -> Result<File, FsError> {
    let task_path = path.clone();

    run_blocking(path, move || lock_handle_validated(&task_path, file)).await
}

/// Acquire a validated lock at the provided path: open the file (via `open`),
/// lock it, then verify the file we locked is still the live one at the path.
/// The lock file may have been removed (and possibly recreated) by another
/// process between our opening it and acquiring the lock; since the lock is
/// bound to the open file rather than the path, we could otherwise be holding
/// a lock on an orphaned file that grants no mutual exclusion. On a stale
/// handle, release, close, and re-acquire against the current file.
fn acquire_validated(
    path: &Path,
    exclusive: bool,
    open: impl Fn(&Path) -> Result<File, FsError>,
) -> Result<File, FsError> {
    loop {
        let file = open(path)?;

        if exclusive {
            acquire_exclusive_lock(path, &file)?;
        } else {
            acquire_shared_lock(path, &file)?;
        }

        if is_lock_current(path, &file)? {
            return Ok(file);
        }

        // Closing the handle releases the lock regardless, so a failed
        // explicit release must not abort the acquisition
        let _ = release_lock(path, &file);

        trace!(file = ?path, "Lock file was replaced while locking, retrying");

        // The stale `file` handle drops here, before the next open. This
        // matters on Windows, where a held handle keeps a delete-pending
        // file's name alive under classic delete semantics.
    }
}

/// Return true if `path` still resolves to the same inode as the locked `file`
/// handle. Because Unix locks are bound to the open file description (inode)
/// and not the path, a lock file that was unlinked — and possibly recreated by
/// another process — leaves us holding a lock on an orphaned inode that grants
/// no mutual exclusion. Comparing the handle's `(dev, ino)` against the path's
/// detects that so the caller can re-acquire.
#[cfg(unix)]
fn is_lock_current(path: &Path, file: &File) -> Result<bool, FsError> {
    use std::os::unix::fs::MetadataExt;

    let locked = file.metadata().map_err(|error| FsError::Read {
        path: path.to_path_buf(),
        error: Box::new(error),
    })?;

    match std::fs::metadata(path) {
        Ok(current) => Ok(locked.dev() == current.dev() && locked.ino() == current.ino()),
        // A missing path means the file we locked was removed
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        // Any other failure (permissions, etc) is persistent — propagate it
        // instead of treating it as stale, which would retry indefinitely
        Err(error) => Err(FsError::Read {
            path: path.to_path_buf(),
            error: Box::new(error),
        }),
    }
}

/// Windows variant of [`is_lock_current`]. Deleting a lock file that other
/// processes have open marks it delete-pending — and with POSIX delete
/// semantics (NTFS on Windows 10+) the name is freed immediately, letting
/// another process create and lock a *separate* file while waiters are still
/// acquiring the doomed one. Instead of re-opening the path (which is
/// ambiguous for delete-pending files), ask the kernel about the handle we
/// actually locked: a delete-pending or zero-link file no longer guards the
/// path, so the caller must re-acquire.
#[cfg(windows)]
fn is_lock_current(path: &Path, file: &File) -> Result<bool, FsError> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_STANDARD_INFO, FileStandardInfo, GetFileInformationByHandleEx,
    };

    let mut info = unsafe { std::mem::zeroed::<FILE_STANDARD_INFO>() };

    let result = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle() as _,
            FileStandardInfo,
            &mut info as *mut FILE_STANDARD_INFO as *mut _,
            std::mem::size_of::<FILE_STANDARD_INFO>() as u32,
        )
    };

    if result == 0 {
        return Err(FsError::Read {
            path: path.to_path_buf(),
            error: Box::new(std::io::Error::last_os_error()),
        });
    }

    // `DeletePending` covers classic delete semantics (FAT, network shares),
    // while a zero link count covers POSIX delete semantics (modern NTFS).
    Ok(!info.DeletePending && info.NumberOfLinks > 0)
}

/// On other platforms an open, locked file cannot be swapped out from under
/// us this way, so the locked handle is always current.
#[cfg(not(any(unix, windows)))]
fn is_lock_current(_path: &Path, _file: &File) -> Result<bool, FsError> {
    Ok(true)
}

/// Instance representing a directory lock.
pub type DirLock = FileLock;

/// Return true if the directory is currently locked (via [`lock_directory`]).
/// Stale `.lock` files are ignored. This is an advisory point-in-time probe;
/// the state may change immediately after it returns.
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
/// This is an advisory point-in-time probe; the state may change immediately
/// after it returns.
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

    // Let other processes know that we have locked it. Content is only
    // written now that the lock is held and validated.
    fs::truncate_file_handle(&lock.path, &mut lock.file)?;

    lock.file
        .write_all(format!("{pid}").as_bytes())
        .map_err(|error| FsError::Write {
            path: lock.path.clone(),
            error: Box::new(error),
        })?;

    Ok(lock)
}

/// Lock the provided file with exclusive access, creating it if it does not
/// exist. The file's content is not modified. If another process currently
/// holds the lock, this will block waiting for it to be unlocked.
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
///
/// The lock is validated after acquisition: if the file was removed or
/// replaced at the path while waiting, an error is returned instead of
/// operating on an orphaned file. Because the file handle is caller-provided,
/// this cannot re-acquire; use [`lock_file`] or [`write_file_with_lock`] for
/// paths that may be removed and recreated concurrently.
#[inline]
#[instrument(skip(file, op))]
pub fn run_with_exclusive_lock<T, F, V>(path: T, mut file: File, op: F) -> Result<V, FsError>
where
    T: AsRef<Path> + Debug,
    F: FnOnce(&mut File) -> Result<V, FsError>,
{
    let path = path.as_ref();

    acquire_exclusive_lock(path, &file)?;

    if !is_lock_current(path, &file)? {
        let _ = release_lock(path, &file);

        return Err(stale_lock_error(path));
    }

    let result = op(&mut file);

    // Release before propagating the operation's error, and let the
    // operation's error take precedence over a release failure
    let released = release_lock(path, &file);
    let value = result?;
    released?;

    Ok(value)
}

/// Lock the provided file with shared access and execute the operation.
///
/// The lock is validated after acquisition: if the file was removed or
/// replaced at the path while waiting, an error is returned instead of
/// operating on an orphaned file. Because the file handle is caller-provided,
/// this cannot re-acquire; use [`read_file_with_lock`] for paths that may be
/// removed and recreated concurrently.
#[inline]
#[instrument(skip(file, op))]
pub fn run_with_shared_lock<T, F, V>(path: T, mut file: File, op: F) -> Result<V, FsError>
where
    T: AsRef<Path> + Debug,
    F: FnOnce(&mut File) -> Result<V, FsError>,
{
    let path = path.as_ref();

    acquire_shared_lock(path, &file)?;

    if !is_lock_current(path, &file)? {
        let _ = release_lock(path, &file);

        return Err(stale_lock_error(path));
    }

    let result = op(&mut file);

    // Release before propagating the operation's error, and let the
    // operation's error take precedence over a release failure
    let released = release_lock(path, &file);
    let value = result?;
    released?;

    Ok(value)
}

fn stale_lock_error(path: &Path) -> FsError {
    FsError::Lock {
        path: path.to_path_buf(),
        error: Box::new(std::io::Error::other(
            "file was removed or replaced while acquiring the lock",
        )),
    }
}

/// Read a file at the provided path into a string, while applying a shared lock.
/// The path must already exist.
#[inline]
pub fn read_file_with_lock<T: AsRef<Path>>(path: T) -> Result<String, FsError> {
    let path = path.as_ref();

    // Re-acquires if the file is removed and recreated while waiting; if it
    // is removed and not recreated, the retried open reports the missing path
    let mut file = acquire_validated(path, false, |path| fs::open_file(path))?;

    let mut buffer = String::new();

    let result = file
        .read_to_string(&mut buffer)
        .map_err(|error| FsError::Read {
            path: path.to_path_buf(),
            error: Box::new(error),
        });

    let released = release_lock(path, &file);
    result?;
    released?;

    Ok(buffer)
}

/// Write a file with the provided data to the provided path, using an exclusive lock.
/// If the parent directory does not exist, it will be created.
#[inline]
pub fn write_file_with_lock<T: AsRef<Path>, D: AsRef<[u8]>>(
    path: T,
    data: D,
) -> Result<(), FsError> {
    let path = path.as_ref();

    // The file is opened without truncating, as we don't own its content
    // until the lock is acquired and validated; truncating on open would
    // corrupt reads made under a still-active lock
    let mut file = acquire_validated(path, true, create_lock_file)?;

    trace!(file = ?path, "Writing file");

    let result = (|| {
        // Truncate then write file
        fs::truncate_file_handle(path, &mut file)?;

        file.write_all(data.as_ref())
            .map_err(|error: std::io::Error| FsError::Write {
                path: path.to_path_buf(),
                error: Box::new(error),
            })
    })();

    let released = release_lock(path, &file);
    result?;
    released?;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("starbase-fs-lock-{}-{name}", std::process::id()))
    }

    // No locks are taken here — this tests file identity tracking only, on
    // both Unix (inode compare) and Windows (delete-pending/link count).
    #[test]
    fn detects_removed_and_replaced_lock_files() {
        let path = temp_path("replaced");
        let _ = std::fs::remove_file(&path);

        let old = create_lock_file(&path).unwrap();

        assert!(is_lock_current(&path, &old).unwrap());

        // Removed: the handle no longer guards the path
        std::fs::remove_file(&path).unwrap();

        assert!(!is_lock_current(&path, &old).unwrap());

        // Replaced: only the new handle guards the path
        let new = create_lock_file(&path).unwrap();

        assert!(!is_lock_current(&path, &old).unwrap());
        assert!(is_lock_current(&path, &new).unwrap());

        drop(old);
        drop(new);

        let _ = std::fs::remove_file(&path);
    }
}
