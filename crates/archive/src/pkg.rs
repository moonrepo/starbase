use crate::archive::ArchiveUnpacker;
use crate::archive_error::ArchiveError;
use crate::helpers::{convert_command_error, copy_extracted_contents, next_temp_dir};
use starbase_utils::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tracing::{instrument, trace};

pub use crate::pkg_error::PkgError;

/// Opens macOS installer packages (`.pkg`) using the system `pkgutil`
/// command. Unlike tar and zip, a package is not read as a stream: the
/// package is expanded into a temporary directory, and the contents of
/// each component's payload are copied to the output directory. Package
/// metadata (`Distribution`, `PackageInfo`, `Bom`, `Scripts`) is not
/// copied, and each component's install location is ignored.
///
/// Only supported on macOS, and only supports unpacking. To create
/// packages, use `pkgbuild` and `productbuild` directly.
pub struct PkgUnpacker {
    archive_file: PathBuf,
    output_dir: PathBuf,
}

impl PkgUnpacker {
    /// Create a new pkg unpacker that expands the archive file and copies
    /// its payload contents to the output directory.
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

        let expand_dir = next_temp_dir();

        // Remove any stale directory left behind by a crashed process
        fs::remove_dir_all(&expand_dir)?;

        expand_pkg(&self.archive_file, &expand_dir).map_err(|error| PkgError::UnpackFailure {
            error: Box::new(error),
        })?;

        let result = find_payload_dirs(&expand_dir).and_then(|payload_dirs| {
            if payload_dirs.is_empty() {
                return Err(PkgError::MissingPayload {
                    path: self.archive_file.clone(),
                }
                .into());
            }

            copy_payloads(
                &payload_dirs,
                &self.output_dir,
                if prefix.is_empty() {
                    None
                } else {
                    Some(prefix)
                },
            )
        });

        let _ = fs::remove_dir_all(&expand_dir);

        result?;

        Ok(self.output_dir)
    }
}

// A flat component package keeps its payload at the root of the package,
// while a distribution package nests component packages, each with their
// own payload, in sub-directories.
fn find_payload_dirs(expand_dir: &Path) -> Result<Vec<PathBuf>, ArchiveError> {
    // A payload must be a real directory: `--expand-full` extracts it as
    // one, while anything else (like a symlink planted in the package)
    // could redirect the copy to an unrelated directory.
    fn is_payload(path: &Path) -> bool {
        path.symlink_metadata().is_ok_and(|meta| meta.is_dir())
    }

    let root_payload = expand_dir.join("Payload");

    if is_payload(&root_payload) {
        return Ok(vec![root_payload]);
    }

    let mut payload_dirs = vec![];

    for entry in fs::read_dir(expand_dir)? {
        let payload_dir = entry.path().join("Payload");

        if is_payload(&payload_dir) {
            payload_dirs.push(payload_dir);
        }
    }

    // Copy payloads in a stable order, since the read order is not guaranteed
    payload_dirs.sort();

    Ok(payload_dirs)
}

fn copy_payloads(
    payload_dirs: &[PathBuf],
    target_dir: &Path,
    prefix: Option<&str>,
) -> Result<(), ArchiveError> {
    let mut copied = false;
    let mut missing = None;

    for payload_dir in payload_dirs {
        match copy_extracted_contents("macOS package", payload_dir, target_dir, prefix) {
            Ok(()) => {
                copied = true;
            }
            Err(error @ ArchiveError::MissingArchiveContents { .. }) => {
                missing = Some(error);
            }
            Err(error) => {
                return Err(error);
            }
        }
    }

    match missing {
        Some(error) if !copied => Err(error),
        _ => Ok(()),
    }
}

fn expand_pkg(archive_file: &Path, expand_dir: &Path) -> Result<(), io::Error> {
    let output = Command::new("pkgutil")
        .arg("--expand-full")
        .arg(archive_file)
        .arg(expand_dir)
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .output()?;

    if output.status.success() {
        return Ok(());
    }

    Err(convert_command_error("pkgutil", &output))
}
