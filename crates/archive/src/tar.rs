use crate::archive::{ArchivePacker, ArchiveUnpacker};
use crate::tree_differ::TreeDiffer;
use miette::Diagnostic;
use starbase_styles::{Style, Stylize};
use starbase_utils::fs;
use std::io::{prelude::*, Write};
use std::path::{Path, PathBuf};
use tar::{Archive as TarArchive, Builder as TarBuilder};
use thiserror::Error;
use tracing::trace;

#[derive(Error, Diagnostic, Debug)]
pub enum TarError {
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

/// Creates tar archives.
pub struct TarPacker {
    archive: TarBuilder<Box<dyn Write>>,
}

impl TarPacker {
    /// Create a new packer with a custom writer.
    pub fn create(writer: Box<dyn Write>) -> miette::Result<Self> {
        Ok(TarPacker {
            archive: TarBuilder::new(writer),
        })
    }

    /// Create a new `.tar` packer.
    pub fn new(output_file: &Path) -> miette::Result<Self> {
        TarPacker::create(Box::new(fs::create_file(output_file)?))
    }

    /// Create a new `.tar.gz` packer.
    #[cfg(feature = "tar-gz")]
    pub fn new_gz(output_file: &Path) -> miette::Result<Self> {
        Self::new_gz_with_level(output_file, 4)
    }

    /// Create a new `.tar.gz` packer with a custom compression level.
    #[cfg(feature = "tar-gz")]
    pub fn new_gz_with_level(output_file: &Path, level: u32) -> miette::Result<Self> {
        TarPacker::create(Box::new(flate2::write::GzEncoder::new(
            fs::create_file(output_file)?,
            flate2::Compression::new(level),
        )))
    }

    /// Create a new `.tar.xz` packer.
    #[cfg(feature = "tar-xz")]
    pub fn new_xz(output_file: &Path) -> miette::Result<Self> {
        Self::new_xz_with_level(output_file, 4)
    }

    /// Create a new `.tar.xz` packer with a custom compression level.
    #[cfg(feature = "tar-xz")]
    pub fn new_xz_with_level(output_file: &Path, level: u32) -> miette::Result<Self> {
        TarPacker::create(Box::new(xz2::write::XzEncoder::new(
            fs::create_file(output_file)?,
            level,
        )))
    }

    /// Create a new `.tar.zstd` packer.
    #[cfg(feature = "tar-zstd")]
    pub fn new_zstd(output_file: &Path) -> miette::Result<Self> {
        Self::new_zstd_with_level(output_file, 4)
    }

    /// Create a new `.tar.zstd` packer with a custom compression level.
    #[cfg(feature = "tar-zstd")]
    pub fn new_zstd_with_level(output_file: &Path, level: u32) -> miette::Result<Self> {
        use miette::IntoDiagnostic;

        TarPacker::create(Box::new(
            zstd::stream::Encoder::new(fs::create_file(output_file)?, level as i32)
                .into_diagnostic()?
                .auto_finish(),
        ))
    }
}

impl ArchivePacker for TarPacker {
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
        trace!(source = name, input = ?dir, "Packing directory");

        self.archive
            .append_dir_all(name, dir)
            .map_err(|error| TarError::AddFailure {
                source: dir.to_path_buf(),
                error,
            })?;

        Ok(())
    }

    fn pack(&mut self) -> miette::Result<()> {
        trace!("Creating tarball");

        self.archive
            .finish()
            .map_err(|error| TarError::PackFailure { error })?;

        Ok(())
    }
}

/// Opens tar archives.
pub struct TarUnpacker {
    archive: TarArchive<Box<dyn Read>>,
    output_dir: PathBuf,
}

impl TarUnpacker {
    /// Create a new unpacker with a custom reader.
    pub fn create(output_dir: &Path, reader: Box<dyn Read>) -> miette::Result<Self> {
        fs::create_dir_all(output_dir)?;

        Ok(TarUnpacker {
            archive: TarArchive::new(reader),
            output_dir: output_dir.to_path_buf(),
        })
    }

    /// Create a new `.tar` unpacker.
    pub fn new(output_dir: &Path, input_file: &Path) -> miette::Result<Self> {
        TarUnpacker::create(output_dir, Box::new(fs::open_file(input_file)?))
    }

    /// Create a new `.tar.gz` unpacker.
    #[cfg(feature = "tar-gz")]
    pub fn new_gz(output_dir: &Path, input_file: &Path) -> miette::Result<Self> {
        TarUnpacker::create(
            output_dir,
            Box::new(flate2::read::GzDecoder::new(fs::open_file(input_file)?)),
        )
    }

    /// Create a new `.tar.xz` unpacker.
    #[cfg(feature = "tar-xz")]
    pub fn new_xz(output_dir: &Path, input_file: &Path) -> miette::Result<Self> {
        TarUnpacker::create(
            output_dir,
            Box::new(xz2::read::XzDecoder::new(fs::open_file(input_file)?)),
        )
    }

    /// Create a new `.tar.zstd` unpacker.
    #[cfg(feature = "tar-zstd")]
    pub fn new_zstd(output_dir: &Path, input_file: &Path) -> miette::Result<Self> {
        use miette::IntoDiagnostic;

        TarUnpacker::create(
            output_dir,
            Box::new(zstd::stream::Decoder::new(fs::open_file(input_file)?).into_diagnostic()?),
        )
    }
}

impl ArchiveUnpacker for TarUnpacker {
    fn unpack(&mut self, prefix: &str, differ: &mut TreeDiffer) -> miette::Result<()> {
        self.archive.set_overwrite(true);

        trace!(output_dir = ?self.output_dir, "Opening tarball");

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
            let output_path = self.output_dir.join(&path);

            if let Some(parent_dir) = output_path.parent() {
                fs::create_dir_all(parent_dir)?;
            }

            trace!(source = ?path, "Unpacking file");

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
