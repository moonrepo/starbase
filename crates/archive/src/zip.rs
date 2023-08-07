use crate::archive::{ArchivePacker, ArchiveUnpacker};
use crate::join_file_name;
use crate::tree_differ::TreeDiffer;
use miette::Diagnostic;
use starbase_styles::{Style, Stylize};
use starbase_utils::fs::{self, FsError};
use std::fs::File;
use std::io::{self, prelude::*};
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::trace;
use zip::write::FileOptions;
use zip::{result::ZipError as BaseZipError, CompressionMethod, ZipArchive, ZipWriter};

#[derive(Error, Diagnostic, Debug)]
pub enum ZipError {
    #[diagnostic(code(zip::pack::add))]
    #[error("Failed to add source {} to archive.", .source.style(Style::Path))]
    AddFailure {
        source: PathBuf,
        #[source]
        error: BaseZipError,
    },

    #[diagnostic(code(zip::unpack::extract))]
    #[error("Failed to extract {} from archive.", .source.style(Style::Path))]
    ExtractFailure {
        source: PathBuf,
        #[source]
        error: std::io::Error,
    },

    #[diagnostic(code(zip::pack::finish))]
    #[error("Failed to pack archive.")]
    PackFailure {
        #[source]
        error: BaseZipError,
    },

    #[diagnostic(code(zip::unpack::finish))]
    #[error("Failed to unpack archive.")]
    UnpackFailure {
        #[source]
        error: BaseZipError,
    },
}

/// Creates zip archives.
pub struct ZipPacker {
    archive: ZipWriter<File>,
}

impl ZipPacker {
    /// Create a new `.zip` packer.
    pub fn new(output_file: &Path) -> miette::Result<Self> {
        Ok(ZipPacker {
            archive: ZipWriter::new(fs::create_file(output_file)?),
        })
    }
}

impl ArchivePacker for ZipPacker {
    fn add_file(&mut self, name: &str, file: &Path) -> miette::Result<()> {
        #[allow(unused_mut)] // windows
        let mut options = FileOptions::default().compression_method(CompressionMethod::Deflated);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            options = options.unix_permissions(fs::metadata(file)?.permissions().mode());
        }

        self.archive
            .start_file(name, options)
            .map_err(|error| ZipError::AddFailure {
                source: file.to_path_buf(),
                error,
            })?;

        self.archive
            .write_all(&fs::read_file_bytes(file)?)
            .map_err(|error| FsError::Write {
                path: file.to_path_buf(),
                error,
            })?;

        Ok(())
    }

    fn add_dir(&mut self, name: &str, dir: &Path) -> miette::Result<()> {
        trace!(source = name, input = ?dir, "Packing directory");

        self.archive
            .add_directory(
                name,
                FileOptions::default().compression_method(CompressionMethod::Deflated),
            )
            .map_err(|error| ZipError::AddFailure {
                source: dir.to_path_buf(),
                error,
            })?;

        let mut dirs = vec![];

        for entry in fs::read_dir(dir)? {
            let path = entry.path();
            let path_suffix = path.strip_prefix(dir).unwrap();
            let name = join_file_name([name, path_suffix.to_str().unwrap()]);

            if path.is_dir() {
                dirs.push((name, path));
            } else {
                self.add_file(&name, &path)?;
            }
        }

        for (name, dir) in dirs {
            self.add_dir(&name, &dir)?;
        }

        Ok(())
    }

    fn pack(&mut self) -> miette::Result<()> {
        trace!("Creating zip");

        self.archive
            .finish()
            .map_err(|error| ZipError::PackFailure { error })?;

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
    pub fn new(output_dir: &Path, input_file: &Path) -> miette::Result<Self> {
        fs::create_dir_all(output_dir)?;

        Ok(ZipUnpacker {
            archive: ZipArchive::new(fs::open_file(input_file)?)
                .map_err(|error| ZipError::UnpackFailure { error })?,
            output_dir: output_dir.to_path_buf(),
        })
    }
}

impl ArchiveUnpacker for ZipUnpacker {
    fn unpack(&mut self, prefix: &str, differ: &mut TreeDiffer) -> miette::Result<()> {
        trace!(output_dir = ?self.output_dir, "Opening zip");

        for i in 0..self.archive.len() {
            let mut file = self
                .archive
                .by_index(i)
                .map_err(|error| ZipError::UnpackFailure { error })?;

            let mut path = match file.enclosed_name() {
                Some(path) => path.to_owned(),
                None => continue,
            };

            // Remove the prefix
            if !prefix.is_empty() && path.starts_with(prefix) {
                path = path.strip_prefix(prefix).unwrap().to_owned();
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
                    error,
                })?;

                fs::update_perms(&output_path, file.unix_mode())?;
            }

            differ.untrack_file(&output_path);
        }

        Ok(())
    }
}
