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
    ) -> miette::Result<TarPacker<xz2::read::XzEncoder<File>>> {
        TarPacker::new(xz2::read::XzEncoder::new(
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
    source_root: PathBuf,
}

impl<R: Read> TarUnpacker<R> {
    pub fn new(source_root: &Path, reader: R) -> miette::Result<Self> {
        fs::create_dir_all(source_root)?;

        Ok(TarUnpacker {
            archive: TarArchive::new(reader),
            source_root: source_root.to_path_buf(),
        })
    }

    pub fn new_raw(source_root: &Path, archive_file: &Path) -> miette::Result<TarUnpacker<File>> {
        TarUnpacker::new(source_root, fs::open_file(archive_file)?)
    }

    #[cfg(feature = "tar-gz")]
    pub fn new_gz(
        source_root: &Path,
        archive_file: &Path,
    ) -> miette::Result<TarUnpacker<flate2::write::GzDecoder<File>>> {
        dbg!("new_gz", source_root, archive_file);

        dbg!(fs::metadata(archive_file).unwrap().len());

        TarUnpacker::new(
            source_root,
            flate2::write::GzDecoder::new(fs::open_file(archive_file)?),
        )
    }

    #[cfg(feature = "tar-xz")]
    pub fn new_xz(
        source_root: &Path,
        archive_file: &Path,
    ) -> miette::Result<TarUnpacker<xz2::read::XzDecoder<File>>> {
        TarUnpacker::new(
            source_root,
            xz2::read::XzDecoder::new(fs::open_file(archive_file)?),
        )
    }
}

impl<R: Read> ArchiveUnpacker for TarUnpacker<R> {
    fn unpack(&mut self, prefix: &str, differ: &mut TreeDiffer) -> miette::Result<()> {
        self.archive.set_overwrite(true);

        dbg!("unpack", &self.source_root);

        for entry in self
            .archive
            .entries()
            .map_err(|error| TarError::UnpackFailure { error })?
        {
            dbg!("entry");
            let mut entry = entry.map_err(|error| TarError::UnpackFailure { error })?;
            let mut path: PathBuf = entry.path().unwrap().into_owned();

            dbg!(3, &path);

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
