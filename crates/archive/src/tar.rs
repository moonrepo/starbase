use crate::archive::{ArchiveItem, ArchivePacker, ArchiveUnpacker};
use crate::error::ArchiveError;
use miette::Diagnostic;
use starbase_styles::{Style, Stylize};
use starbase_utils::fs::{self, FsError};
use std::fs::File;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use tar::{Archive as TarArchive, Builder as TarBuilder, Entries, Entry};
use thiserror::Error;

#[derive(Error, Diagnostic, Debug)]
pub enum TarError {
    #[error(transparent)]
    Fs(#[from] FsError),

    #[diagnostic(code(tar::pack::add))]
    #[error("Failed to add source {} to archive", .source.style(Style::Path))]
    AddFailure {
        source: PathBuf,
        #[source]
        error: std::io::Error,
    },

    #[diagnostic(code(tar::unpack::extract))]
    #[error("Failed to extract {} from archive", .source.style(Style::Path))]
    ExtractFailure {
        source: PathBuf,
        #[source]
        error: std::io::Error,
    },

    #[diagnostic(code(tar::pack::finish))]
    #[error("Failed to pack archive")]
    PackFailure {
        #[source]
        error: std::io::Error,
    },

    #[diagnostic(code(tar::unpack::finish))]
    #[error("Failed to unpack archive")]
    UnpackFailure {
        #[source]
        error: std::io::Error,
    },
}

pub struct TarPacker<W: Write> {
    archive: TarBuilder<W>,
}

impl<W: Write> TarPacker<W> {
    pub fn new(writer: W) -> Result<Self, TarError> {
        Ok(TarPacker {
            archive: TarBuilder::new(writer),
        })
    }

    #[cfg(feature = "tar-gz")]
    pub fn new_gz<P>(
        archive_file: P,
        level: Option<u32>,
    ) -> Result<TarPacker<flate2::write::GzEncoder<File>>, TarError>
    where
        P: AsRef<Path>,
    {
        TarPacker::new(flate2::write::GzEncoder::new(
            fs::create_file(archive_file.as_ref())?,
            flate2::Compression::new(level.unwrap_or(4)),
        ))
    }

    #[cfg(feature = "tar-xz")]
    pub fn new_xz<P>(
        archive_file: P,
        level: Option<u32>,
    ) -> Result<TarPacker<xz2::read::XzEncoder<File>>, TarError>
    where
        P: AsRef<Path>,
    {
        TarPacker::new(xz2::read::XzEncoder::new(
            fs::create_file(archive_file.as_ref())?,
            level.unwrap_or(4),
        ))
    }
}

impl<W: Write> ArchivePacker for TarPacker<W> {
    fn add_file(&mut self, name: &str, file: &Path) -> Result<(), ArchiveError> {
        self.archive
            .append_file(name, &mut fs::open_file(file)?)
            .map_err(|error| TarError::AddFailure {
                source: file.to_path_buf(),
                error,
            })?;

        Ok(())
    }

    fn add_dir(&mut self, name: &str, dir: &Path) -> Result<(), ArchiveError> {
        self.archive
            .append_dir_all(name, dir)
            .map_err(|error| TarError::AddFailure {
                source: dir.to_path_buf(),
                error,
            })?;

        Ok(())
    }

    fn pack(&mut self) -> Result<(), ArchiveError> {
        self.archive
            .finish()
            .map_err(|error| TarError::PackFailure { error })?;

        Ok(())
    }
}

pub struct TarUnpacker<'archive, R: Read> {
    archive: TarArchive<R>,
    _marker: std::marker::PhantomData<&'archive ()>,
}

impl<'archive, R: Read> TarUnpacker<'archive, R> {
    pub fn new(reader: R) -> Result<Self, TarError> {
        Ok(TarUnpacker {
            archive: TarArchive::new(reader),
            _marker: std::marker::PhantomData,
        })
    }

    #[cfg(feature = "tar-gz")]
    pub fn new_gz<P>(
        archive_file: P,
    ) -> Result<TarUnpacker<'archive, flate2::write::GzDecoder<File>>, TarError>
    where
        P: AsRef<Path>,
    {
        TarUnpacker::new(flate2::write::GzDecoder::new(fs::create_file(
            archive_file.as_ref(),
        )?))
    }

    #[cfg(feature = "tar-xz")]
    pub fn new_xz<P>(
        archive_file: P,
    ) -> Result<TarUnpacker<'archive, xz2::read::XzDecoder<File>>, TarError>
    where
        P: AsRef<Path>,
    {
        TarUnpacker::new(xz2::read::XzDecoder::new(fs::create_file(
            archive_file.as_ref(),
        )?))
    }
}

impl<'archive, R: Read + 'archive> ArchiveUnpacker for TarUnpacker<'archive, R> {
    type Item = TarItem<'archive, R>;
    type Iterator = TarItemIterator<'archive, R>;

    fn unpack(&mut self) -> Result<(), ArchiveError> {
        Ok(())
    }

    fn contents(&mut self) -> Result<Self::Iterator, ArchiveError> {
        Ok(TarItemIterator {
            entries: self
                .archive
                .entries()
                .map_err(|error| TarError::UnpackFailure { error })?,
        })
    }
}

pub struct TarItemIterator<'archive, R: Read> {
    entries: Entries<'archive, R>,
}

impl<'archive, R: Read> Iterator for TarItemIterator<'archive, R> {
    type Item = Result<TarItem<'archive, R>, ArchiveError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.entries.next().map(|entry| {
            entry
                .map(|e| TarItem { entry: e })
                .map_err(|error| ArchiveError::Tar(TarError::UnpackFailure { error }))
        })
    }
}

pub struct TarItem<'archive, R: Read> {
    entry: Entry<'archive, R>,
}

impl<'archive, R: Read> ArchiveItem for TarItem<'archive, R> {
    fn create(&mut self, dest: &Path) -> Result<(), ArchiveError> {
        if let Some(parent_dir) = dest.parent() {
            fs::create_dir_all(parent_dir)?;
        }

        self.entry
            .unpack(dest)
            .map_err(|error| TarError::ExtractFailure {
                source: dest.to_path_buf(),
                error,
            })?;

        Ok(())
    }

    fn path(&self) -> PathBuf {
        self.entry.path().unwrap().into_owned()
    }

    fn size(&self) -> u64 {
        self.entry.size()
    }
}

impl<'archive, R: Read> Read for TarItem<'archive, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.entry.read(buf)
    }
}
