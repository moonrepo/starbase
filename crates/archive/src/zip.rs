use crate::archive::{ArchivePacker, ArchiveUnpacker};
use crate::archive_error::ArchiveError;
use crate::helpers::{escapes_via_symlink, join_file_name};
use starbase_utils::fs::{self, FsError};
use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};
use tracing::{instrument, trace};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub use crate::zip_error::ZipError;

/// Creates zip archives by writing to the provided stream.
///
/// Unlike tar, zip does not compose with codecs: compression is part of
/// the zip format itself and is applied per entry, so it's configured
/// with [`ZipPacker::with_compression`] instead. The stream must also be
/// seekable, since the format ends with a central directory.
pub struct ZipPacker<W: Write + Seek> {
    archive: ZipWriter<W>,
    compression: CompressionMethod,
}

impl<W: Write + Seek> ZipPacker<W> {
    /// Create a new zip packer with no compression (entries are stored).
    pub fn new(writer: W) -> Self {
        Self::with_compression(writer, CompressionMethod::Stored)
    }

    /// Create a new zip packer with a custom compression method. The
    /// matching `zip-*` Cargo feature must be enabled for the method.
    pub fn with_compression(writer: W, compression: CompressionMethod) -> Self {
        ZipPacker {
            archive: ZipWriter::new(writer),
            compression,
        }
    }
}

impl<W: Write + Seek> ArchivePacker for ZipPacker<W> {
    fn add_file(&mut self, name: &str, file: &Path) -> Result<(), ArchiveError> {
        trace!(source = name, input = ?file, "Adding file");

        #[allow(unused_mut)] // windows
        let mut options = SimpleFileOptions::default().compression_method(self.compression);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            options = options.unix_permissions(fs::metadata(file)?.permissions().mode());
        }

        self.archive
            .start_file(name, options)
            .map_err(|error| ZipError::AddFailure {
                source: file.to_path_buf(),
                error: Box::new(error),
            })?;

        let mut input = fs::open_file(file)?;

        io::copy(&mut input, &mut self.archive).map_err(|error| FsError::Write {
            path: file.to_path_buf(),
            error: Box::new(error),
        })?;

        Ok(())
    }

    fn add_dir(&mut self, name: &str, dir: &Path) -> Result<(), ArchiveError> {
        trace!(source = name, input = ?dir, "Adding directory");

        self.archive
            .add_directory(
                name,
                SimpleFileOptions::default().compression_method(self.compression),
            )
            .map_err(|error| ZipError::AddFailure {
                source: dir.to_path_buf(),
                error: Box::new(error),
            })?;

        let mut dirs = vec![];

        for entry in fs::read_dir(dir)? {
            if let Ok(file_type) = entry.file_type() {
                let path = entry.path();
                let path_suffix = path.strip_prefix(dir).unwrap();
                let name = join_file_name([name, path_suffix.to_str().unwrap()]);

                if file_type.is_dir() {
                    dirs.push((name, path));
                } else {
                    self.add_file(&name, &path)?;
                }
            }
        }

        for (name, dir) in dirs {
            self.add_dir(&name, &dir)?;
        }

        Ok(())
    }

    #[instrument(name = "pack_zip", skip_all)]
    fn pack(self) -> Result<(), ArchiveError> {
        trace!("Packing zip");

        // Writes the central directory and returns the stream.
        let mut writer = self
            .archive
            .finish()
            .map_err(|error| ZipError::PackFailure {
                error: Box::new(error),
            })?;

        writer.flush().map_err(|error| ZipError::PackFailure {
            error: Box::new(error.into()),
        })?;

        Ok(())
    }
}

/// Opens zip archives by reading from the provided stream. The stream
/// must be seekable, since the format ends with a central directory.
pub struct ZipUnpacker<R: Read + Seek> {
    archive: ZipArchive<R>,
    output_dir: PathBuf,
}

impl<R: Read + Seek> ZipUnpacker<R> {
    /// Create a new zip unpacker that reads from the provided stream and
    /// extracts to the output directory.
    pub fn new(output_dir: &Path, reader: R) -> Result<Self, ArchiveError> {
        Ok(ZipUnpacker {
            archive: ZipArchive::new(reader).map_err(|error| ZipError::UnpackFailure {
                error: Box::new(error),
            })?,
            output_dir: output_dir.to_path_buf(),
        })
    }
}

impl<R: Read + Seek> ArchiveUnpacker for ZipUnpacker<R> {
    #[instrument(name = "unpack_zip", skip_all)]
    fn unpack(mut self, prefix: &str) -> Result<PathBuf, ArchiveError> {
        fs::create_dir_all(&self.output_dir)?;

        trace!(output_dir = ?self.output_dir, "Unpacking zip");

        let mut count = 0;

        for i in 0..self.archive.len() {
            let mut file = self
                .archive
                .by_index(i)
                .map_err(|error| ZipError::UnpackFailure {
                    error: Box::new(error),
                })?;

            let mut path = match file.enclosed_name() {
                Some(path) => path.to_owned(),
                None => continue,
            };

            // Remove the prefix
            if !prefix.is_empty()
                && let Ok(suffix) = path.strip_prefix(prefix)
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

            // If a folder, create the dir
            if file.is_dir() {
                fs::create_dir_all(&output_path)?;
            }

            // If a file, copy it to the output dir
            if file.is_file() {
                let mut out = fs::create_file(&output_path)?;

                io::copy(&mut file, &mut out).map_err(|error| ZipError::ExtractFailure {
                    source: output_path.to_path_buf(),
                    error: Box::new(error),
                })?;

                // Only apply the archive's stored mode. Defaulting a missing
                // mode to 0o755 would mark every file from a mode-less (e.g.
                // Windows-created) zip as executable.
                if let Some(mode) = file.unix_mode() {
                    fs::update_perms(&output_path, Some(mode))?;
                }
            }

            count += 1;
        }

        trace!("Unpacked {count} files");

        Ok(self.output_dir)
    }
}
