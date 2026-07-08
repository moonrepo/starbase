use crate::archive::ArchiveUnpacker;
use crate::archive_error::ArchiveError;
use crate::helpers::copy_extracted_contents;
use starbase_utils::fs;
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::Duration;
use tracing::{instrument, trace};

pub use crate::dmg_error::DmgError;

pub struct DmgUnpacker {
    archive_file: PathBuf,
    output_dir: PathBuf,
}

impl DmgUnpacker {
    pub fn new(output_dir: impl AsRef<Path>, archive_file: impl AsRef<Path>) -> Self {
        DmgUnpacker {
            archive_file: archive_file.as_ref().to_path_buf(),
            output_dir: output_dir.as_ref().to_path_buf(),
        }
    }
}

impl ArchiveUnpacker for DmgUnpacker {
    #[instrument(name = "unpack_dmg", skip_all)]
    fn unpack(self, prefix: &str) -> Result<PathBuf, ArchiveError> {
        trace!(output_dir = ?self.output_dir, "Unpacking dmg archive");

        let temp_dir = env::temp_dir();
        let mount_dir = temp_dir.join("dmg");

        fs::create_dir_all(temp_dir)?;
        fs::remove_dir_all(&mount_dir)?;

        let result = {
            attach_dmg(&self.archive_file, &mount_dir).map_err(|error| {
                DmgError::UnpackFailure {
                    error: Box::new(error),
                }
            })?;

            if !mount_dir.exists() {
                return Err(DmgError::MissingVolume {
                    path: mount_dir.to_path_buf(),
                }
                .into());
            }

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
        };

        // Always detach the volume, even if extracting the contents failed
        let _ = detach_dmg(&mount_dir);
        let _ = fs::remove_dir_all(&mount_dir);

        result?;

        Ok(self.output_dir)
    }
}

fn attach_dmg(archive_file: &Path, mount_dir: &Path) -> Result<(), std::io::Error> {
    // macOS DiskArbitration only permits a limited number of concurrent attach
    // operations. When proto installs multiple tools in parallel, `hdiutil attach`
    // can transiently fail with "Resource temporarily unavailable", so retry a
    // handful of times with a backoff before giving up.
    let max_attempts = 5;
    let mut attempt = 1;

    loop {
        let result = Command::new("hdiutil")
            .arg("attach")
            .arg(archive_file)
            .arg("-nobrowse")
            .arg("-readonly")
            .arg("-noautoopen")
            .arg("-mountpoint")
            .arg(mount_dir)
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .output();

        match result {
            Ok(_) => return Ok(()),
            Err(error) => {
                if attempt >= max_attempts {
                    return Err(error.into());
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
    }
}

fn detach_dmg(mount_dir: &Path) -> Result<(), std::io::Error> {
    Command::new("hdiutil")
        .arg("detach")
        .arg(mount_dir)
        .arg("-force")
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .output()?;

    Ok(())
}
