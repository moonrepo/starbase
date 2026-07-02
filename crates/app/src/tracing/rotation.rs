use crate::tracing::{TracingError, TracingResult};
use std::fs;
use std::path::{Path, PathBuf};

/// Options for rotating log files.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct LogFileRotation {
    /// Number of rotated log files to keep. Set to `0` to disable rotation.
    pub max_files: usize,
}

impl Default for LogFileRotation {
    fn default() -> Self {
        Self { max_files: 7 }
    }
}

fn rotated_log_file_path(log_file: &Path, index: usize) -> PathBuf {
    let name = log_file
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_else(|| "app");
    let ext = log_file
        .extension()
        .and_then(|name| name.to_str())
        .unwrap_or_else(|| "log");

    log_file.with_file_name(format!("{name}.{index}.{ext}"))
}

fn try_log_file_exists(path: &Path) -> TracingResult<bool> {
    path.try_exists()
        .map_err(|error| TracingError::RotateLogFileFailed {
            from: path.to_owned(),
            to: None,
            error,
        })
}

pub fn rotate_log_file(log_file: &Path, rotation: LogFileRotation) -> TracingResult<()> {
    if rotation.max_files == 0 || !try_log_file_exists(log_file)? {
        return Ok(());
    }

    let oldest_log_file = rotated_log_file_path(log_file, rotation.max_files);

    // Remove the oldest log file if it exists
    if try_log_file_exists(&oldest_log_file)? {
        fs::remove_file(&oldest_log_file).map_err(|error| TracingError::RotateLogFileFailed {
            from: oldest_log_file,
            to: None,
            error,
        })?;
    }

    // Rotate the existing log files by renaming them to the next index
    for index in (1..rotation.max_files).rev() {
        let from = rotated_log_file_path(log_file, index);

        if try_log_file_exists(&from)? {
            let to = rotated_log_file_path(log_file, index + 1);

            fs::rename(&from, &to).map_err(|error| TracingError::RotateLogFileFailed {
                from,
                to: Some(to),
                error,
            })?;
        }
    }

    // Rename the current log file to the first rotated log file
    let rotated_log_file = rotated_log_file_path(log_file, 1);

    fs::rename(log_file, &rotated_log_file).map_err(|error| TracingError::RotateLogFileFailed {
        from: log_file.to_owned(),
        to: Some(rotated_log_file),
        error,
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::time::SystemTime;

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = env::temp_dir().join(format!(
                "starbase-{name}-{}",
                SystemTime::UNIX_EPOCH
                    .elapsed()
                    .expect("system clock should be after unix epoch")
                    .as_nanos()
            ));

            fs::create_dir_all(&path).expect("temp directory should be created");

            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn rotates_existing_log_files() {
        let temp_dir = TempDir::new("rotates-existing-log-files");
        let log_file = temp_dir.path().join("starbase.log");

        fs::write(&log_file, "current").expect("current log file should be written");
        fs::write(rotated_log_file_path(&log_file, 1), "previous")
            .expect("previous log file should be written");
        fs::write(rotated_log_file_path(&log_file, 2), "older")
            .expect("older log file should be written");
        fs::write(rotated_log_file_path(&log_file, 3), "oldest")
            .expect("oldest log file should be written");

        rotate_log_file(&log_file, LogFileRotation { max_files: 3 })
            .expect("log files should rotate");

        assert!(!log_file.exists());
        assert_eq!(
            fs::read_to_string(rotated_log_file_path(&log_file, 1))
                .expect("rotated current log file should exist"),
            "current"
        );
        assert_eq!(
            fs::read_to_string(rotated_log_file_path(&log_file, 2))
                .expect("rotated previous log file should exist"),
            "previous"
        );
        assert_eq!(
            fs::read_to_string(rotated_log_file_path(&log_file, 3))
                .expect("rotated older log file should exist"),
            "older"
        );
        assert!(!rotated_log_file_path(&log_file, 4).exists());
    }

    #[test]
    fn leaves_log_file_when_rotation_is_disabled() {
        let temp_dir = TempDir::new("leaves-log-file-when-rotation-is-disabled");
        let log_file = temp_dir.path().join("starbase.log");

        fs::write(&log_file, "current").expect("current log file should be written");

        rotate_log_file(&log_file, LogFileRotation { max_files: 0 })
            .expect("disabled rotation should succeed");

        assert_eq!(
            fs::read_to_string(log_file).expect("current log file should still exist"),
            "current"
        );
    }

    #[test]
    fn appends_rotation_index_to_file_name() {
        let log_file = PathBuf::from("/tmp/starbase.log");

        assert_eq!(
            rotated_log_file_path(&log_file, 12),
            PathBuf::from("/tmp/starbase.12.log")
        );
    }
}
