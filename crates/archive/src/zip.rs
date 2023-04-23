use crate::join_file_name;
use crate::{archive::ArchivePacker, ArchiveError};
use miette::Diagnostic;
use starbase_styles::{Style, Stylize};
use starbase_utils::fs::{self, FsError};
use std::fs::File;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use thiserror::Error;
use zip::write::FileOptions;
use zip::{result::ZipError as BaseZipError, CompressionMethod, ZipWriter};

#[derive(Error, Diagnostic, Debug)]
pub enum ZipError {
    #[error(transparent)]
    Fs(#[from] FsError),

    #[diagnostic(code(zip::pack::add))]
    #[error("Failed to add source {} to archive", .source.style(Style::Path))]
    AddFailure {
        source: PathBuf,
        #[source]
        error: BaseZipError,
    },

    #[diagnostic(code(zip::pack::finish))]
    #[error("Failed to pack archive")]
    PackFailure {
        #[source]
        error: BaseZipError,
    },
}

pub struct ZipPacker {
    archive: ZipWriter<File>,
}

impl ZipPacker {
    pub fn new<P>(archive_file: P) -> Result<Self, ArchiveError>
    where
        P: AsRef<Path>,
    {
        Ok(ZipPacker {
            archive: ZipWriter::new(fs::create_file(archive_file.as_ref())?),
        })
    }
}

impl ArchivePacker for ZipPacker {
    type Error = ZipError;

    fn add_file(&mut self, name: &str, file: &Path) -> Result<(), Self::Error> {
        #[allow(unused_mut)] // windows
        let mut options = FileOptions::default().compression_method(CompressionMethod::Stored);

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
            .write_all(fs::read_file(file)?.as_bytes())
            .map_err(|error| {
                ZipError::Fs(FsError::Write {
                    path: file.to_path_buf(),
                    error,
                })
            })?;

        Ok(())
    }

    fn add_dir(&mut self, name: &str, dir: &Path) -> Result<(), Self::Error> {
        self.archive
            .add_directory(
                name,
                FileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .map_err(|error| ZipError::AddFailure {
                source: dir.to_path_buf(),
                error,
            })?;

        for entry in fs::read_dir(dir)? {
            let path = entry.path();
            let path_suffix = path.strip_prefix(dir).unwrap();
            let name = join_file_name(&[name, path_suffix.to_str().unwrap()]);

            if path.is_dir() {
                self.add_dir(&name, &path)?;
            } else {
                self.add_file(&name, &path)?;
            }
        }

        Ok(())
    }

    fn pack(&mut self) -> Result<(), Self::Error> {
        self.archive
            .finish()
            .map_err(|error| ZipError::PackFailure { error })?;

        Ok(())
    }
}
