use crate::archive::{ArchivePacker, ArchiveUnpacker};
use crate::error::ArchiveError;
use crate::tree_differ::TreeDiffer;
use miette::Diagnostic;
use starbase_styles::{Style, Stylize};
use starbase_utils::fs::{self, FsError};
use std::fs::File;
use std::io::{prelude::*, Write};
use std::path::{Path, PathBuf};
use tar::{Archive as TarArchive, Builder as TarBuilder};
use thiserror::Error;

#[derive(Error, Diagnostic, Debug)]
pub enum TarError {
    #[diagnostic(transparent)]
    #[error(transparent)]
    Fs(#[from] FsError),

    #[diagnostic(code(tar::pack::add))]
    #[error("Failed to add source {} to archive.", .source.style(Style::Path))]
    AddFailure {
        source: PathBuf,
        #[source]
        error: std::io::Error,
    },

    #[diagnostic(code(tar::unpack::extract))]
    #[error("Failed to extract {} from archive.", .source.style(Style::Path))]
    ExtractFailure {
        source: PathBuf,
        #[source]
        error: std::io::Error,
    },

    #[diagnostic(code(tar::pack::finish))]
    #[error("Failed to pack archive.")]
    PackFailure {
        #[source]
        error: std::io::Error,
    },

    #[diagnostic(code(tar::unpack::finish))]
    #[error("Failed to unpack archive.")]
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

pub struct TarUnpacker<R: Read> {
    archive: TarArchive<R>,
    source_root: PathBuf,
}

impl<R: Read> TarUnpacker<R> {
    pub fn new(source_root: &Path, reader: R) -> Result<Self, TarError> {
        Ok(TarUnpacker {
            archive: TarArchive::new(reader),
            source_root: source_root.to_path_buf(),
        })
    }

    #[cfg(feature = "tar-gz")]
    pub fn new_gz<P>(
        source_root: &Path,
        archive_file: &Path,
    ) -> Result<TarUnpacker<flate2::write::GzDecoder<File>>, TarError> {
        TarUnpacker::new(
            source_root,
            flate2::write::GzDecoder::new(fs::create_file(archive_file)?),
        )
    }

    #[cfg(feature = "tar-xz")]
    pub fn new_xz<P>(
        source_root: &Path,
        archive_file: &Path,
    ) -> Result<TarUnpacker<xz2::read::XzDecoder<File>>, TarError> {
        TarUnpacker::new(
            source_root,
            xz2::read::XzDecoder::new(fs::create_file(archive_file)?),
        )
    }
}

impl<R: Read> ArchiveUnpacker for TarUnpacker<R> {
    fn unpack(&mut self, prefix: &str, differ: &mut TreeDiffer) -> Result<(), ArchiveError> {
        self.archive.set_overwrite(true);

        for entry in self
            .archive
            .entries()
            .map_err(|error| ArchiveError::Tar(TarError::UnpackFailure { error }))?
        {
            let mut entry =
                entry.map_err(|error| ArchiveError::Tar(TarError::UnpackFailure { error }))?;
            let mut path: PathBuf = entry.path().unwrap().into_owned();

            // Remove the prefix
            if !prefix.is_empty() && path.starts_with(prefix) {
                path = path.strip_prefix(prefix).unwrap().to_owned();
            }

            // Unpack the file if different than destination
            let output_path = self.source_root.join(path);

            if let Some(parent_dir) = output_path.parent() {
                fs::create_dir_all(parent_dir)?;
            }

            // NOTE: gzip doesn't support seeking, so we can't use the following util then!
            // if differ.should_write_source(entry.size(), &mut entry, &output_path)? {
            entry
                .unpack(&output_path)
                .map_err(|error| TarError::ExtractFailure {
                    source: output_path.clone(),
                    error,
                })?;
            // }

            differ.untrack_file(&output_path);
        }

        Ok(())
    }
}
