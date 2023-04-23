use crate::archive::ArchivePacker;
use miette::Diagnostic;
use starbase_styles::{Style, Stylize};
use starbase_utils::fs::{self, FsError};
use std::fs::File;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use tar::Builder as TarBuilder;
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

    #[diagnostic(code(tar::pack::finish))]
    #[error("Failed to pack archive")]
    PackFailure {
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
    type Error = TarError;

    fn add_file(&mut self, name: &str, file: &Path) -> Result<(), Self::Error> {
        self.archive
            .append_file(name, &mut fs::open_file(file)?)
            .map_err(|error| TarError::AddFailure {
                source: file.to_path_buf(),
                error,
            })?;

        Ok(())
    }

    fn add_dir(&mut self, name: &str, dir: &Path) -> Result<(), Self::Error> {
        self.archive
            .append_dir_all(name, dir)
            .map_err(|error| TarError::AddFailure {
                source: dir.to_path_buf(),
                error,
            })?;

        Ok(())
    }

    fn pack(&mut self) -> Result<(), Self::Error> {
        self.archive
            .finish()
            .map_err(|error| TarError::PackFailure { error })?;

        Ok(())
    }
}
