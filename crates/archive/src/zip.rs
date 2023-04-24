use crate::archive::{ArchiveItem, ArchivePacker, ArchiveUnpacker};
use crate::error::ArchiveError;
use crate::join_file_name;
use miette::Diagnostic;
use starbase_styles::{Style, Stylize};
use starbase_utils::fs::{self, FsError};
use std::fs::File;
use std::io::{self, prelude::*};
use std::path::{Path, PathBuf};
use thiserror::Error;
use zip::read::ZipFile;
use zip::write::FileOptions;
use zip::{result::ZipError as BaseZipError, CompressionMethod, ZipArchive, ZipWriter};

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

    #[diagnostic(code(zip::unpack::extract))]
    #[error("Failed to extract {} from archive", .source.style(Style::Path))]
    ExtractFailure {
        source: PathBuf,
        #[source]
        error: std::io::Error,
    },

    #[diagnostic(code(zip::pack::finish))]
    #[error("Failed to pack archive")]
    PackFailure {
        #[source]
        error: BaseZipError,
    },

    #[diagnostic(code(zip::unpack::finish))]
    #[error("Failed to unpack archive")]
    UnpackFailure {
        #[source]
        error: BaseZipError,
    },
}

pub struct ZipPacker {
    archive: ZipWriter<File>,
}

impl ZipPacker {
    pub fn new<P>(archive_file: P) -> Result<Self, ZipError>
    where
        P: AsRef<Path>,
    {
        Ok(ZipPacker {
            archive: ZipWriter::new(fs::create_file(archive_file.as_ref())?),
        })
    }
}

impl ArchivePacker for ZipPacker {
    fn add_file(&mut self, name: &str, file: &Path) -> Result<(), ArchiveError> {
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

    fn add_dir(&mut self, name: &str, dir: &Path) -> Result<(), ArchiveError> {
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

    fn pack(&mut self) -> Result<(), ArchiveError> {
        self.archive
            .finish()
            .map_err(|error| ZipError::PackFailure { error })?;

        Ok(())
    }
}

pub struct ZipUnpacker<'archive> {
    archive: ZipArchive<File>,
    _marker: std::marker::PhantomData<&'archive ()>,
}

impl<'archive> ZipUnpacker<'archive> {
    pub fn new<P>(archive_file: P) -> Result<Self, ZipError>
    where
        P: AsRef<Path>,
    {
        Ok(ZipUnpacker {
            archive: ZipArchive::new(fs::open_file(archive_file.as_ref())?)
                .map_err(|error| ZipError::UnpackFailure { error })?,
            _marker: std::marker::PhantomData,
        })
    }
}

// impl<'archive> ArchiveUnpacker for ZipUnpacker<'archive> {
//     type Item = ZipUnpackerEntry<'archive>;

//     fn unpack(&mut self) -> Result<(), ArchiveError> {
//         Ok(())
//     }

//     fn contents(&mut self) -> Result<Vec<Self::Content>, ArchiveError> {
//         let mut entries = vec![];

//         // for i in 0..self.archive.len() {
//         //     entries.push(ZipUnpackerEntry {
//         //         entry: self
//         //             .archive
//         //             .by_index(i)
//         //             .map_err(|error| ZipError::UnpackFailure { error })?,
//         //     });
//         // }

//         Ok(entries)
//     }
// }

pub struct ZipUnpackerEntry<'archive> {
    entry: ZipFile<'archive>,
}

impl<'archive> ArchiveItem for ZipUnpackerEntry<'archive> {
    fn create(&mut self, dest: &Path) -> Result<(), ArchiveError> {
        if let Some(parent_dir) = dest.parent() {
            fs::create_dir_all(parent_dir)?;
        }

        // If a folder, create the dir
        if self.entry.is_dir() {
            fs::create_dir_all(&dest)?;
        }

        // If a file, copy it to the output dir
        if self.entry.is_file() {
            let mut out = fs::create_file(&dest)?;

            io::copy(&mut self.entry, &mut out).map_err(|error| ZipError::ExtractFailure {
                source: dest.to_path_buf(),
                error,
            })?;

            fs::update_perms(&dest, self.entry.unix_mode())?;
        }

        Ok(())
    }

    fn path(&self) -> PathBuf {
        self.entry.enclosed_name().unwrap().to_path_buf()
    }

    fn size(&self) -> u64 {
        self.entry.size()
    }
}

impl<'archive> Read for ZipUnpackerEntry<'archive> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.entry.read(buf)
    }
}
