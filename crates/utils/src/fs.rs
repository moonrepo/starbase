use std::ffi::OsStr;
use std::fmt::Debug;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tracing::{instrument, trace};

pub use crate::fs_error::FsError;
#[cfg(feature = "fs-lock")]
pub use crate::fs_lock::*;

/// Append a file with the provided content. If the parent directory does not exist,
/// or the file to append does not exist, they will be created.
#[inline]
#[instrument(skip(data))]
pub fn append_file<T: AsRef<Path> + Debug, D: AsRef<[u8]>>(
    path: T,
    data: D,
) -> Result<(), FsError> {
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
pub fn copy_dir_all<R: AsRef<Path> + Debug, F: AsRef<Path> + Debug, T: AsRef<Path> + Debug>(
    from_root: R,
    from: F,
    to_root: T,
) -> Result<(), FsError> {
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
        if let Ok(file_type) = entry.file_type() {
            if file_type.is_file() {
                let path = entry.path();

                copy_file(&path, to_root.join(path.strip_prefix(from_root).unwrap()))?;
            } else if file_type.is_dir() {
                dirs.push(entry.path());
            }
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

/// Detect the indentation of the provided string, by scanning and comparing each line.
#[instrument(skip(content))]
pub fn detect_indentation<T: AsRef<str>>(content: T) -> String {
    let mut spaces = 0;
    let mut tabs = 0;
    let mut lowest_space_width = 0;
    let mut lowest_tab_width = 0;

    fn count_line_indent(line: &str, indent: char) -> usize {
        let mut line_count = 0;
        let mut line_check = line;

        while let Some(inner) = line_check.strip_prefix(indent) {
            line_count += 1;
            line_check = inner;
        }

        line_count
    }

    for line in content.as_ref().lines() {
        if line.starts_with(' ') {
            let line_spaces = count_line_indent(line, ' ');

            // Throw out odd numbers so comments don't throw us
            if line_spaces % 2 == 1 {
                continue;
            }

            spaces += 1;

            if lowest_space_width == 0 || line_spaces < lowest_space_width {
                lowest_space_width = line_spaces;
            }
        } else if line.starts_with('\t') {
            let line_tabs = count_line_indent(line, '\t');

            tabs += 1;

            if lowest_tab_width == 0 || line_tabs < lowest_tab_width {
                lowest_tab_width = line_tabs;
            }
        } else {
            continue;
        }
    }

    if spaces > tabs {
        " ".repeat(lowest_space_width)
    } else {
        "\t".repeat(lowest_tab_width)
    }
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
#[instrument]
pub fn get_editor_config_props<T: AsRef<Path> + Debug>(
    path: T,
) -> Result<EditorConfigProps, FsError> {
    use ec4rs::property::*;

    let path = path.as_ref();
    let editor_config = ec4rs::properties_of(path).unwrap_or_default();
    let tab_width = editor_config
        .get::<TabWidth>()
        .unwrap_or(TabWidth::Value(4));
    let indent_size = editor_config
        .get::<IndentSize>()
        .unwrap_or(IndentSize::Value(2));
    let indent_style = editor_config.get::<IndentStyle>().ok();
    let insert_final_newline = editor_config
        .get::<FinalNewline>()
        .unwrap_or(FinalNewline::Value(true));

    Ok(EditorConfigProps {
        eof: if matches!(insert_final_newline, FinalNewline::Value(true)) {
            "\n".into()
        } else {
            "".into()
        },
        indent: match indent_style {
            Some(IndentStyle::Tabs) => "\t".into(),
            Some(IndentStyle::Spaces) => match indent_size {
                IndentSize::UseTabWidth => match tab_width {
                    TabWidth::Value(value) => " ".repeat(value),
                },
                IndentSize::Value(value) => " ".repeat(value),
            },
            None => {
                if path.exists() {
                    detect_indentation(read_file(path)?)
                } else {
                    "  ".into()
                }
            }
        },
    })
}

/// Check if the provided path is a stale file, by comparing modified, created, or accessed
/// timestamps against the current timestamp and duration. If stale, return the file size
/// and timestamp, otherwise return `None`.
#[inline]
#[instrument]
pub fn is_stale<T: AsRef<Path> + Debug>(
    path: T,
    accessed: bool,
    duration: Duration,
    current_time: SystemTime,
) -> Result<Option<(u64, SystemTime)>, FsError> {
    let path = path.as_ref();

    // Avoid bubbling up result errors and just mark as stale
    if let Ok(meta) = metadata(path) {
        let mut time = meta.modified().or_else(|_| meta.created());

        if accessed {
            if let Ok(accessed_time) = meta.accessed() {
                time = Ok(accessed_time);
            }
        }

        if let Ok(check_time) = time {
            if check_time < (current_time - duration) {
                return Ok(Some((meta.len(), check_time)));
            }
        }
    }

    Ok(None)
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

/// Open a file at the provided path and return a [`File`] instance.
/// The path must already exist.
#[inline]
#[instrument]
pub fn open_file<T: AsRef<Path> + Debug>(path: T) -> Result<File, FsError> {
    let path = path.as_ref();

    trace!(file = ?path, "Opening file");

    File::open(path).map_err(|error| FsError::Read {
        path: path.to_path_buf(),
        error: Box::new(error),
    })
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
    if let Ok(metadata) = path.symlink_metadata() {
        if metadata.is_symlink() {
            trace!(file = ?path, "Removing symlink");

            fs::remove_file(path).map_err(|error| FsError::Remove {
                path: path.to_path_buf(),
                error: Box::new(error),
            })?;
        }
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
    current_time: SystemTime,
) -> Result<u64, FsError> {
    let path = path.as_ref();

    if path.exists() {
        if let Some((size, _)) = is_stale(path, true, duration, current_time)? {
            trace!(file = ?path, "Removing stale file");

            fs::remove_file(path).map_err(|error| FsError::Remove {
                path: path.to_path_buf(),
                error: Box::new(error),
            })?;

            return Ok(size);
        }
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
    let now = SystemTime::now();

    trace!(
        dir = ?dir,
        "Removing stale contents from directory"
    );

    for entry in read_dir_all(dir)? {
        if entry.file_type().is_ok_and(|file_type| file_type.is_file()) {
            if let Ok(bytes) = remove_file_if_stale(entry.path(), duration, now) {
                if bytes > 0 {
                    files_deleted += 1;
                    bytes_saved += bytes;
                }
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
pub fn write_file<T: AsRef<Path> + Debug, D: AsRef<[u8]>>(path: T, data: D) -> Result<(), FsError> {
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

/// Write a file with the provided data to the provided path, while taking the
/// closest `.editorconfig` into account
#[cfg(feature = "editor-config")]
#[inline]
#[instrument(skip(data))]
pub fn write_file_with_config<T: AsRef<Path> + Debug, D: AsRef<[u8]>>(
    path: T,
    data: D,
) -> Result<(), FsError> {
    let path = path.as_ref();
    let editor_config = get_editor_config_props(path)?;

    let mut data = unsafe { String::from_utf8_unchecked(data.as_ref().to_vec()) };
    editor_config.apply_eof(&mut data);

    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }

    trace!(file = ?path, "Writing file with .editorconfig");

    fs::write(path, data).map_err(|error| FsError::Write {
        path: path.to_path_buf(),
        error: Box::new(error),
    })
}
