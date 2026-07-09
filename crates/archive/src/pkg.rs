use crate::archive::ArchiveUnpacker;
use crate::archive_error::ArchiveError;
use crate::helpers::copy_extracted_contents;
use crate::next_mount_dir;
use starbase_utils::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tracing::{instrument, trace};

pub use crate::pkg_error::PkgError;

pub struct PkgUnpacker {
    archive_file: PathBuf,
    output_dir: PathBuf,
}

impl PkgUnpacker {
    pub fn new(output_dir: &Path, archive_file: &Path) -> Self {
        PkgUnpacker {
            archive_file: archive_file.to_path_buf(),
            output_dir: output_dir.to_path_buf(),
        }
    }
}

impl ArchiveUnpacker for PkgUnpacker {
    #[instrument(name = "unpack_pkg", skip_all)]
    fn unpack(self, prefix: &str) -> Result<PathBuf, ArchiveError> {
        fs::create_dir_all(&self.output_dir)?;

        trace!(output_dir = ?self.output_dir, "Unpacking pkg");

        let mount_dir = next_mount_dir();
        let payload_dir = mount_dir.join("Payload");

        // Remove any stale mount point left behind by a crashed process
        fs::remove_dir_all(&mount_dir)?;

        unpack_pkg(&self.archive_file, &mount_dir).map_err(|error| PkgError::UnpackFailure {
            error: Box::new(error),
        })?;

        let result = if payload_dir.exists() {
            copy_extracted_contents(
                "macOS package",
                &mount_dir,
                &self.output_dir,
                if prefix.is_empty() {
                    None
                } else {
                    Some(prefix)
                },
            )
        } else {
            Err(PkgError::MissingPayload {
                path: payload_dir.to_path_buf(),
            }
            .into())
        };

        let _ = fs::remove_dir_all(&mount_dir);

        result?;

        Ok(self.output_dir)
    }
}

fn unpack_pkg(archive_file: &Path, mount_dir: &Path) -> Result<(), io::Error> {
    let output = Command::new("pkgutil")
        .arg("--expand-full")
        .arg(archive_file)
        .arg(&mount_dir)
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .output()?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    Err(io::Error::other(if stderr.is_empty() {
        format!("pkgutil failed ({})", output.status)
    } else {
        stderr
    }))
}
