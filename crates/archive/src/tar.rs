use crate::archive::{ArchivePacker, ArchiveUnpacker};
use crate::archive_error::ArchiveError;
use crate::codecs::Finish;
use crate::helpers::{escapes_via_symlink, strip_path_prefix};
use jdx_tar::{
    Archive as TarArchive, Builder as TarBuilder, Entry as TarEntry, EntryType, EntryUnpacker,
    Header as TarHeader, UnpackOptions,
};
use starbase_utils::fs;
use std::fs::{File, Metadata, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;
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

    fn append_file(&mut self, name: &Path, file: &Path, metadata: &Metadata) -> io::Result<()> {
        let mut header = create_header(EntryType::File, metadata);
        header.set_size(metadata.len());

        self.archive
            .append_data(&mut header, name, File::open(file)?)
    }

    fn append_dir(&mut self, name: &Path, metadata: &Metadata) -> io::Result<()> {
        let mut header = create_header(EntryType::Directory, metadata);

        self.archive.append_data(&mut header, name, io::empty())
    }

    // Like `append_dir_all` from the `tar` crate, symlinks are followed,
    // and directories are written before their contents. Children are
    // sorted so that archives are reproducible.
    fn append_tree(&mut self, name: &Path, source: &Path) -> io::Result<()> {
        let metadata = std::fs::metadata(source)?;

        if metadata.is_dir() {
            if !name.as_os_str().is_empty() {
                self.append_dir(name, &metadata)?;
            }

            let mut children = std::fs::read_dir(source)?
                .map(|entry| entry.map(|entry| entry.path()))
                .collect::<io::Result<Vec<_>>>()?;

            children.sort();

            for child in children {
                if let Some(file_name) = child.file_name() {
                    self.append_tree(&name.join(file_name), &child)?;
                }
            }
        } else if metadata.is_file() {
            self.append_file(name, source, &metadata)?;
        } else {
            trace!(source = ?source, "Skipping special file");
        }

        Ok(())
    }
}

fn create_header(kind: EntryType, metadata: &Metadata) -> TarHeader {
    let mut header = TarHeader::new_gnu(kind);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        header.set_mode(metadata.permissions().mode() & 0o7777);
    }

    #[cfg(not(unix))]
    header.set_mode(if metadata.is_dir() {
        0o755
    } else if metadata.permissions().readonly() {
        0o444
    } else {
        0o644
    });

    if let Ok(modified) = metadata.modified()
        && let Ok(elapsed) = modified.duration_since(UNIX_EPOCH)
        && let Ok(seconds) = i64::try_from(elapsed.as_secs())
    {
        header.set_mtime(seconds);
    }

    header
}

impl<W: Write + Finish> ArchivePacker for TarPacker<W> {
    fn add_file(&mut self, name: &str, file: &Path) -> Result<(), ArchiveError> {
        trace!(source = name, input = ?file, "Adding file");

        std::fs::metadata(file)
            .and_then(|metadata| self.append_file(Path::new(name), file, &metadata))
            .map_err(|error| TarError::AddFailure {
                source: file.to_path_buf(),
                error: Box::new(error),
            })?;

        Ok(())
    }

    fn add_dir(&mut self, name: &str, dir: &Path) -> Result<(), ArchiveError> {
        trace!(source = name, input = ?dir, "Adding directory");

        self.append_tree(Path::new(name), dir)
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
    // A `jdx_tar` archive is not `Send`, so it's created when unpacking.
    reader: R,
    output_dir: PathBuf,
}

impl<R: Read> TarUnpacker<R> {
    /// Create a new tar unpacker that reads from the provided stream and
    /// extracts to the output directory.
    pub fn new(output_dir: &Path, reader: R) -> Self {
        TarUnpacker {
            reader,
            output_dir: output_dir.to_path_buf(),
        }
    }
}

// Clean the entry path by removing prefix, root, and current directory
// components, and resolving parent directory components. Returns `None`
// if the path escapes the output directory or is empty.
fn safe_entry_path(path: &Path) -> Option<PathBuf> {
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

// Whether `jdx_tar` can extract the entry at its own path. It rejects
// absolute paths and parent directory components, instead of resolving
// them like `safe_entry_path` does.
fn is_plain_path(path: &Path) -> bool {
    path.components()
        .all(|part| matches!(part, Component::Normal(_) | Component::CurDir))
}

// Extract an entry at its resolved path, which `jdx_tar` cannot do. Only
// directories and files are supported, since link targets can't be
// resolved soundly relative to such a path. Returns `false` if skipped.
fn unpack_resolved_entry<R: Read>(entry: &mut TarEntry<R>, output_path: &Path) -> io::Result<bool> {
    match entry.entry_type() {
        EntryType::Directory => {
            std::fs::create_dir_all(output_path)?;
        }
        EntryType::File => {
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            // Replace an existing file instead of writing through it
            if let Err(error) = std::fs::remove_file(output_path)
                && error.kind() != io::ErrorKind::NotFound
            {
                return Err(error);
            }

            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output_path)?;

            io::copy(entry, &mut file)?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;

                file.set_permissions(std::fs::Permissions::from_mode(
                    entry.header().mode() & 0o777,
                ))?;
            }
        }
        _ => return Ok(false),
    }

    Ok(true)
}

impl<R: Read> ArchiveUnpacker for TarUnpacker<R> {
    #[instrument(name = "unpack_tar", skip_all)]
    fn unpack(self, prefix: &str) -> Result<PathBuf, ArchiveError> {
        fs::create_dir_all(&self.output_dir)?;

        trace!(output_dir = ?self.output_dir, "Unpacking tarball");

        let unpack_error = |error: io::Error| TarError::UnpackFailure {
            error: Box::new(error),
        };

        // `jdx_tar` refuses to extract into a symlink, which the output
        // directory may be, so extract into its real path instead.
        let root = std::fs::canonicalize(&self.output_dir).map_err(unpack_error)?;

        // Entries within the prefix have its components stripped, while
        // other entries keep their path, so each needs its own unpacker.
        // Both check for traversal and symlinks against the file system,
        // not their own state, so they can share the output directory.
        let mut kept_options = UnpackOptions::default();
        let mut stripped_options = UnpackOptions::default();
        stripped_options.strip_components = Path::new(prefix).components().count();

        let mut kept = EntryUnpacker::new(&root, &mut kept_options).map_err(unpack_error)?;
        let mut stripped =
            EntryUnpacker::new(&root, &mut stripped_options).map_err(unpack_error)?;

        let mut archive = TarArchive::new(self.reader);
        #[cfg(unix)]
        let mut special_modes = vec![];
        let mut count = 0;

        for entry in archive.entries().map_err(unpack_error)? {
            let mut entry = entry.map_err(unpack_error)?;
            let entry_path = entry.path().map_err(unpack_error)?.into_owned();

            // Skip unpacking if the entry is unsafe (avoid zip slips!)
            let Some(path) = safe_entry_path(&entry_path) else {
                trace!(source = ?entry_path, "Skipping entry that would escape the output directory");
                continue;
            };

            // Remove the prefix
            let suffix = if prefix.is_empty() {
                None
            } else {
                strip_path_prefix(&path, prefix)
            };

            let output_path = self.output_dir.join(suffix.unwrap_or(&path));

            // Refuse to write through a symlink planted by an earlier entry,
            // which could redirect the write outside the output directory.
            if escapes_via_symlink(&self.output_dir, &output_path) {
                trace!(source = ?path, "Skipping entry that would escape via a symlink");
                continue;
            }

            let extract_error = |error: io::Error| TarError::ExtractFailure {
                source: output_path.clone(),
                error: Box::new(error),
            };

            let unpacked = if is_plain_path(&entry_path) {
                let unpacker = if suffix.is_some() {
                    &mut stripped
                } else {
                    &mut kept
                };

                let summary = unpacker.unpack(&mut entry).map_err(extract_error)?;

                for skipped in summary.skipped {
                    trace!(source = ?skipped.path, reason = ?skipped.reason, "Skipped entry");
                }

                summary.files + summary.dirs + summary.symlinks + summary.hardlinks > 0
            } else {
                unpack_resolved_entry(&mut entry, &output_path).map_err(extract_error)?
            };

            if !unpacked {
                continue;
            }

            // Permission bits are restored, but setuid, setgid, and sticky
            // bits were never applied before, so don't start now.
            #[cfg(unix)]
            {
                let mode = entry.header().mode();

                if mode & 0o7000 != 0
                    && matches!(entry.entry_type(), EntryType::File | EntryType::Directory)
                {
                    special_modes.push((output_path, mode & 0o777));
                }
            }

            count += 1;
        }

        // Applies deferred directory permissions and timestamps
        kept.finish().map_err(unpack_error)?;
        stripped.finish().map_err(unpack_error)?;

        #[cfg(unix)]
        for (path, mode) in special_modes {
            use std::os::unix::fs::PermissionsExt;

            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).map_err(
                |error| TarError::ExtractFailure {
                    source: path.clone(),
                    error: Box::new(error),
                },
            )?;
        }

        trace!("Unpacked {count} files");

        Ok(self.output_dir)
    }
}
