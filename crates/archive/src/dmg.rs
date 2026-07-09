use crate::archive::ArchiveUnpacker;
use crate::archive_error::ArchiveError;
use crate::helpers::{convert_command_error, copy_extracted_contents, next_temp_dir};
use starbase_utils::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::Duration;
use tracing::{instrument, trace};

pub use crate::dmg_error::DmgError;

/// Opens macOS disk images (`.dmg`) using the system `hdiutil` command.
/// Unlike tar and zip, a disk image is not read as a stream: the image
/// is attached (mounted) as a read-only volume, the volume's contents
/// are copied to the output directory, and the volume is detached.
///
/// Only supported on macOS, and only supports unpacking. To create disk
/// images, use `hdiutil create` directly.
pub struct DmgUnpacker {
    archive_file: PathBuf,
    output_dir: PathBuf,
}

impl DmgUnpacker {
    /// Create a new dmg unpacker that mounts the archive file and copies
    /// the volume's contents to the output directory.
    pub fn new(output_dir: &Path, archive_file: &Path) -> Self {
        DmgUnpacker {
            archive_file: archive_file.to_path_buf(),
            output_dir: output_dir.to_path_buf(),
        }
    }
}

impl ArchiveUnpacker for DmgUnpacker {
    #[instrument(name = "unpack_dmg", skip_all)]
    fn unpack(self, prefix: &str) -> Result<PathBuf, ArchiveError> {
        fs::create_dir_all(&self.output_dir)?;

        trace!(output_dir = ?self.output_dir, "Unpacking dmg");

        let mount_dir = next_temp_dir();

        // Remove any stale mount point left behind by a crashed process
        fs::remove_dir_all(&mount_dir)?;

        attach_dmg(&self.archive_file, &mount_dir).map_err(|error| DmgError::UnpackFailure {
            error: Box::new(error),
        })?;

        let result = if mount_dir.exists() {
            copy_extracted_contents(
                "macOS disk image",
                &mount_dir,
                &self.output_dir,
                if prefix.is_empty() {
                    None
                } else {
                    Some(prefix)
                },
            )
        } else {
            Err(DmgError::MissingVolume {
                path: self.archive_file.to_path_buf(),
            }
            .into())
        };

        // Always detach the volume, even if extracting the contents failed
        if let Err(error) = detach_dmg(&mount_dir) {
            trace!(
                mount_dir = ?mount_dir,
                error = error.to_string(),
                "Failed to detach macOS disk image",
            );
        }

        let _ = fs::remove_dir_all(&mount_dir);

        result?;

        Ok(self.output_dir)
    }
}

fn attach_dmg(archive_file: &Path, mount_dir: &Path) -> Result<(), io::Error> {
    // macOS DiskArbitration only permits a limited number of concurrent
    // attach operations. When multiple images are unpacked in parallel,
    // `hdiutil attach` can transiently fail with "Resource temporarily
    // unavailable", so retry a handful of times with a backoff.
    let max_attempts = 5;
    let mut attempt = 1;

    loop {
        // The mount point does not need to exist, `hdiutil` creates it
        let output = Command::new("hdiutil")
            .arg("attach")
            .arg(archive_file)
            .arg("-nobrowse")
            .arg("-readonly")
            .arg("-noautoopen")
            .arg("-mountpoint")
            .arg(mount_dir)
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .output()?;

        if output.status.success() {
            return Ok(());
        }

        let error = convert_command_error("hdiutil attach", &output);

        // Only transient failures are worth retrying, anything else,
        // like an invalid or corrupt image, will never succeed
        if attempt >= max_attempts
            || !error
                .to_string()
                .to_lowercase()
                .contains("temporarily unavailable")
        {
            return Err(error);
        }

        trace!(
            archive = ?archive_file,
            attempt,
            error = error.to_string(),
            "Failed to attach macOS disk image, retrying",
        );

        sleep(Duration::from_millis(250 * attempt));
        attempt += 1;
    }
}

fn detach_dmg(mount_dir: &Path) -> Result<(), io::Error> {
    let output = Command::new("hdiutil")
        .arg("detach")
        .arg(mount_dir)
        .arg("-force")
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .output()?;

    if output.status.success() {
        return Ok(());
    }

    Err(convert_command_error("hdiutil detach", &output))
}
