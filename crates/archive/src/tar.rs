use crate::archive::{ArchivePacker, ArchiveUnpacker};
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

    #[diagnostic(code(tar::pack))]
    #[error("Failed to pack archive.")]
    PackFailure {
        #[source]
        error: std::io::Error,
    },

    #[diagnostic(code(tar::unpack))]
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
    pub fn new(writer: W) -> miette::Result<Self> {
        Ok(TarPacker {
            archive: TarBuilder::new(writer),
        })
    }

    pub fn new_raw(archive_file: &Path) -> miette::Result<TarPacker<File>> {
        TarPacker::new(fs::create_file(archive_file)?)
    }

    #[cfg(feature = "tar-gz")]
    pub fn new_gz(
        archive_file: &Path,
        level: Option<u32>,
    ) -> miette::Result<TarPacker<flate2::write::GzEncoder<File>>> {
        TarPacker::new(flate2::write::GzEncoder::new(
            fs::create_file(archive_file)?,
            flate2::Compression::new(level.unwrap_or(4)),
        ))
    }

    #[cfg(feature = "tar-xz")]
    pub fn new_xz(
        archive_file: &Path,
        level: Option<u32>,
    ) -> miette::Result<TarPacker<xz2::write::XzEncoder<File>>> {
        TarPacker::new(xz2::write::XzEncoder::new(
            fs::create_file(archive_file)?,
            level.unwrap_or(4),
        ))
    }
}

impl<W: Write> ArchivePacker for TarPacker<W> {
    fn add_file(&mut self, name: &str, file: &Path) -> miette::Result<()> {
        self.archive
            .append_file(name, &mut fs::open_file(file)?)
            .map_err(|error| TarError::AddFailure {
                source: file.to_path_buf(),
                error,
            })?;

        Ok(())
    }

    fn add_dir(&mut self, name: &str, dir: &Path) -> miette::Result<()> {
        self.archive
            .append_dir_all(name, dir)
            .map_err(|error| TarError::AddFailure {
                source: dir.to_path_buf(),
                error,
            })?;

        Ok(())
    }

    fn pack(&mut self) -> miette::Result<()> {
        self.archive
            .finish()
            .map_err(|error| TarError::PackFailure { error })?;

        Ok(())
    }
}

pub struct TarUnpacker<R: Read> {
    archive: TarArchive<R>,
    output_dir: PathBuf,
}

impl<R: Read> TarUnpacker<R> {
    pub fn new(output_dir: &Path, reader: R) -> miette::Result<Self> {
        fs::create_dir_all(output_dir)?;

        Ok(TarUnpacker {
            archive: TarArchive::new(reader),
            output_dir: output_dir.to_path_buf(),
        })
    }

    pub fn new_raw(output_dir: &Path, archive_file: &Path) -> miette::Result<TarUnpacker<File>> {
        TarUnpacker::new(output_dir, fs::open_file(archive_file)?)
    }

    #[cfg(feature = "tar-gz")]
    pub fn new_gz(
        output_dir: &Path,
        archive_file: &Path,
    ) -> miette::Result<TarUnpacker<flate2::read::GzDecoder<File>>> {
        TarUnpacker::new(
            output_dir,
            flate2::read::GzDecoder::new(fs::open_file(archive_file)?),
        )
    }

    #[cfg(feature = "tar-xz")]
    pub fn new_xz(
        output_dir: &Path,
        archive_file: &Path,
    ) -> miette::Result<TarUnpacker<xz2::read::XzDecoder<File>>> {
        TarUnpacker::new(
            output_dir,
            xz2::read::XzDecoder::new(fs::open_file(archive_file)?),
        )
    }
}

impl<R: Read> ArchiveUnpacker for TarUnpacker<R> {
    fn unpack(&mut self, prefix: &str, differ: &mut TreeDiffer) -> miette::Result<()> {
        self.archive.set_overwrite(true);

        for entry in self
            .archive
            .entries()
            .map_err(|error| TarError::UnpackFailure { error })?
        {
            let mut entry = entry.map_err(|error| TarError::UnpackFailure { error })?;
            let mut path: PathBuf = entry.path().unwrap().into_owned();

            // Remove the prefix
            if !prefix.is_empty() && path.starts_with(prefix) {
                path = path.strip_prefix(prefix).unwrap().to_owned();
            }

            // Unpack the file if different than destination
            let output_path = self.output_dir.join(path);

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
