use crate::archive::{ArchivePacker, ArchiveResult, ArchiveUnpacker};
use crate::join_file_name;
use crate::tree_differ::TreeDiffer;
use starbase_utils::fs::{self, FsError};
use std::fs::File;
use std::io::{self, prelude::*};
use std::path::{Path, PathBuf};
use tracing::{instrument, trace};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub use crate::zip_error::ZipError;

/// Creates zip archives.
pub struct ZipPacker {
    archive: ZipWriter<File>,
    compression: CompressionMethod,
}

impl ZipPacker {
    /// Create a new packer with a custom compression level.
    pub fn create(output_file: &Path, compression: CompressionMethod) -> ArchiveResult<Self> {
        Ok(ZipPacker {
            archive: ZipWriter::new(fs::create_file(output_file)?),
            compression,
        })
    }

    /// Create a new `.zip` packer.
    pub fn new(output_file: &Path) -> ArchiveResult<Self> {
        Self::create(output_file, CompressionMethod::Stored)
    }

    /// Create a new compressed `.zip` packer using `deflate`.
    #[cfg(feature = "zip-deflate")]
    pub fn new_deflate(output_file: &Path) -> ArchiveResult<Self> {
        Self::create(output_file, CompressionMethod::Deflated)
    }
}

impl ArchivePacker for ZipPacker {
    fn add_file(&mut self, name: &str, file: &Path) -> ArchiveResult<()> {
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

        self.archive
            .write_all(&fs::read_file_bytes(file)?)
            .map_err(|error| FsError::Write {
                path: file.to_path_buf(),
                error: Box::new(error),
            })?;

        Ok(())
    }

    fn add_dir(&mut self, name: &str, dir: &Path) -> ArchiveResult<()> {
        trace!(source = name, input = ?dir, "Packing directory");

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
    fn pack(&mut self) -> ArchiveResult<()> {
        trace!("Creating zip");

        // Upstream API changed where finish consumes self.
        // Commented this out for now, but it's ok since it also runs on drop.

        // self.archive
        //     .finish()
        //     .map_err(|error| ZipError::PackFailure {
        //         error: Box::new(error),
        //     })?;

        Ok(())
    }
}

/// Opens zip archives.
pub struct ZipUnpacker {
    archive: ZipArchive<File>,
    output_dir: PathBuf,
}

impl ZipUnpacker {
    /// Create a new `.zip` unpacker.
    pub fn new(output_dir: &Path, input_file: &Path) -> ArchiveResult<Self> {
        fs::create_dir_all(output_dir)?;

        Ok(ZipUnpacker {
            archive: ZipArchive::new(fs::open_file(input_file)?).map_err(|error| {
                ZipError::UnpackFailure {
                    error: Box::new(error),
                }
            })?,
            output_dir: output_dir.to_path_buf(),
        })
    }

    /// Create a new `.zip` unpacker for `deflate`.
    #[cfg(feature = "zip-deflate")]
    pub fn new_deflate(output_dir: &Path, input_file: &Path) -> ArchiveResult<Self> {
        Self::new(output_dir, input_file)
    }
}

impl ArchiveUnpacker for ZipUnpacker {
    #[instrument(name = "unpack_zip", skip_all)]
    fn unpack(&mut self, prefix: &str, differ: &mut TreeDiffer) -> ArchiveResult<PathBuf> {
        trace!(output_dir = ?self.output_dir, "Opening zip");

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
            if !prefix.is_empty() {
                if let Ok(suffix) = path.strip_prefix(prefix) {
                    path = suffix.to_owned();
                }
            }

            let output_path = self.output_dir.join(&path);

            // If a folder, create the dir
            if file.is_dir() {
                fs::create_dir_all(&output_path)?;
            }

            // If a file, copy it to the output dir
            // if file.is_file() && differ.should_write_source(file.size(), &mut file, &output_path)? {
            if file.is_file() {
                let mut out = fs::create_file(&output_path)?;

                io::copy(&mut file, &mut out).map_err(|error| ZipError::ExtractFailure {
                    source: output_path.to_path_buf(),
                    error: Box::new(error),
                })?;

                fs::update_perms(&output_path, file.unix_mode())?;
            }

            differ.untrack_file(&output_path);
            count += 1;
        }

        trace!("Unpacked {} files", count);

        Ok(self.output_dir.clone())
    }
}
