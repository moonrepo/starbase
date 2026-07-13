use crate::archive::{ArchivePacker, ArchiveUnpacker};
use crate::archive_error::ArchiveError;
use crate::codecs::Finish;
use crate::helpers::{escapes_via_symlink, strip_path_prefix};
use binstall_tar::{Archive as TarArchive, Builder as TarBuilder, Entry as TarEntry};
use starbase_utils::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use tracing::{instrument, trace};

pub use crate::tar_error::TarError;

/// Creates tar archives by writing to the provided stream. Compose the
/// stream with a codec to create compressed tarballs.
///
/// ```ignore
/// TarPacker::new(fs::create_file(path)?);              // .tar
/// TarPacker::new(Gz::new(fs::create_file(path)?));     // .tar.gz
/// TarPacker::new(Zstd::new(fs::create_file(path)?));   // .tar.zst
/// ```
pub struct TarPacker<W: Write> {
    archive: TarBuilder<W>,
}

impl<W: Write> TarPacker<W> {
    /// Create a new tar packer that writes to the provided stream.
    pub fn new(writer: W) -> Self {
        TarPacker {
            archive: TarBuilder::new(writer),
        }
    }
}

impl<W: Write + Finish> ArchivePacker for TarPacker<W> {
    fn add_file(&mut self, name: &str, file: &Path) -> Result<(), ArchiveError> {
        trace!(source = name, input = ?file, "Adding file");

        self.archive
            .append_file(name, &mut fs::open_file(file)?)
            .map_err(|error| TarError::AddFailure {
                source: file.to_path_buf(),
                error: Box::new(error),
            })?;

        Ok(())
    }

    fn add_dir(&mut self, name: &str, dir: &Path) -> Result<(), ArchiveError> {
        trace!(source = name, input = ?dir, "Adding directory");

        self.archive
            .append_dir_all(name, dir)
            .map_err(|error| TarError::AddFailure {
                source: dir.to_path_buf(),
                error: Box::new(error),
            })?;

        Ok(())
    }

    #[instrument(name = "pack_tar", skip_all)]
    fn pack(self) -> Result<(), ArchiveError> {
        trace!("Packing tarball");

        // Writes the tar footer and returns the stream.
        let mut writer = self
            .archive
            .into_inner()
            .map_err(|error| TarError::PackFailure {
                error: Box::new(error),
            })?;

        // Writes codec epilogues through the entire stream chain.
        writer.finish().map_err(|error| TarError::PackFailure {
            error: Box::new(error),
        })?;

        Ok(())
    }
}

/// Opens tar archives by reading from the provided stream. Compose the
/// stream with a codec to open compressed tarballs.
///
/// ```ignore
/// TarUnpacker::new(dir, fs::open_file(path)?);            // .tar
/// TarUnpacker::new(dir, Gz::new(fs::open_file(path)?));   // .tar.gz
/// TarUnpacker::new(dir, Zstd::new(fs::open_file(path)?)); // .tar.zst
/// ```
pub struct TarUnpacker<R: Read> {
    archive: TarArchive<R>,
    output_dir: PathBuf,
}

impl<R: Read> TarUnpacker<R> {
    /// Create a new tar unpacker that reads from the provided stream and
    /// extracts to the output directory.
    pub fn new(output_dir: &Path, reader: R) -> Self {
        TarUnpacker {
            archive: TarArchive::new(reader),
            output_dir: output_dir.to_path_buf(),
        }
    }
}

// Validations inspired from binstall_tar::Entry::unpack_in().
//
// Didn't use unpack_in() directly for safety as it cannot handle
// our prefix (stripping) related requirement since it considers
// the entry as read-only.
fn safe_entry_path<R: Read>(entry: &TarEntry<R>) -> Option<PathBuf> {
    let path = entry.path().ok()?;
    let mut clean_path = PathBuf::new();
    let mut normal_parts = 0;

    for part in path.components() {
        match part {
            Component::Prefix(..) | Component::RootDir | Component::CurDir => continue,

            Component::ParentDir => {
                // Avoid zip slip vulnerability (CWE-23: Relative Path Traversal)
                if normal_parts == 0 {
                    return None;
                }

                clean_path.pop();
                normal_parts -= 1;
            }

            Component::Normal(part) => {
                clean_path.push(part);
                normal_parts += 1;
            }
        }
    }

    // Original path consisted of only prefix, rootdir, or curdir
    // components (could be a valid directory, but not a valid file path)
    if clean_path.as_os_str().is_empty() {
        return None;
    }

    Some(clean_path)
}

impl<R: Read> ArchiveUnpacker for TarUnpacker<R> {
    #[instrument(name = "unpack_tar", skip_all)]
    fn unpack(mut self, prefix: &str) -> Result<PathBuf, ArchiveError> {
        fs::create_dir_all(&self.output_dir)?;

        self.archive.set_overwrite(true);

        trace!(output_dir = ?self.output_dir, "Unpacking tarball");

        let mut count = 0;

        for entry in self
            .archive
            .entries()
            .map_err(|error| TarError::UnpackFailure {
                error: Box::new(error),
            })?
        {
            let mut entry = entry.map_err(|error| TarError::UnpackFailure {
                error: Box::new(error),
            })?;

            // Skip unpacking if the entry is unsafe (avoid zip slips!)
            let mut path = match safe_entry_path(&entry) {
                Some(path) => path,
                None => continue,
            };

            // Remove the prefix
            if !prefix.is_empty()
                && let Some(suffix) = strip_path_prefix(&path, prefix)
            {
                path = suffix.to_owned();
            }

            let output_path = self.output_dir.join(&path);

            // Refuse to write through a symlink planted by an earlier entry,
            // which could redirect the write outside the output directory.
            if escapes_via_symlink(&self.output_dir, &output_path) {
                trace!(source = ?path, "Skipping entry that would escape via a symlink");
                continue;
            }

            if let Some(parent_dir) = output_path.parent() {
                fs::create_dir_all(parent_dir)?;
            }

            entry
                .unpack(&output_path)
                .map_err(|error| TarError::ExtractFailure {
                    source: output_path.clone(),
                    error: Box::new(error),
                })?;

            count += 1;
        }

        trace!("Unpacked {count} files");

        Ok(self.output_dir)
    }
}
