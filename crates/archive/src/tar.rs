use crate::archive::{ArchivePacker, ArchiveResult, ArchiveUnpacker};
use crate::tree_differ::TreeDiffer;
use binstall_tar::{Archive as TarArchive, Builder as TarBuilder};
use starbase_utils::fs;
use std::io::{Write, prelude::*};
use std::path::{Path, PathBuf};
use tracing::{instrument, trace};

pub use crate::tar_error::TarError;

/// Creates tar archives.
pub struct TarPacker {
    archive: TarBuilder<Box<dyn Write>>,
}

impl TarPacker {
    /// Create a new packer with a custom writer.
    pub fn create(writer: Box<dyn Write>) -> ArchiveResult<Self> {
        Ok(TarPacker {
            archive: TarBuilder::new(writer),
        })
    }

    /// Create a new `.tar` packer.
    pub fn new(output_file: &Path) -> ArchiveResult<Self> {
        TarPacker::create(Box::new(fs::create_file(output_file)?))
    }

    /// Create a new `.tar.gz` packer.
    #[cfg(feature = "tar-gz")]
    pub fn new_gz(output_file: &Path) -> ArchiveResult<Self> {
        Self::new_gz_with_level(output_file, 4)
    }

    /// Create a new `.tar.gz` packer with a custom compression level.
    #[cfg(feature = "tar-gz")]
    pub fn new_gz_with_level(output_file: &Path, level: u32) -> ArchiveResult<Self> {
        TarPacker::create(Box::new(flate2::write::GzEncoder::new(
            fs::create_file(output_file)?,
            flate2::Compression::new(level),
        )))
    }

    /// Create a new `.tar.xz` packer.
    #[cfg(feature = "tar-xz")]
    pub fn new_xz(output_file: &Path) -> ArchiveResult<Self> {
        Self::new_xz_with_level(output_file, 4)
    }

    /// Create a new `.tar.xz` packer with a custom compression level.
    #[cfg(feature = "tar-xz")]
    pub fn new_xz_with_level(output_file: &Path, level: u32) -> ArchiveResult<Self> {
        TarPacker::create(Box::new(liblzma::write::XzEncoder::new(
            fs::create_file(output_file)?,
            level,
        )))
    }

    /// Create a new `.tar.zstd` packer.
    #[cfg(feature = "tar-zstd")]
    pub fn new_zstd(output_file: &Path) -> ArchiveResult<Self> {
        Self::new_zstd_with_level(output_file, 3) // Default in lib
    }

    /// Create a new `.tar.zstd` packer with a custom compression level.
    #[cfg(feature = "tar-zstd")]
    pub fn new_zstd_with_level(output_file: &Path, level: u32) -> ArchiveResult<Self> {
        let encoder = zstd::stream::Encoder::new(fs::create_file(output_file)?, level as i32);

        #[cfg(feature = "miette")]
        {
            use miette::IntoDiagnostic;

            TarPacker::create(Box::new(encoder.into_diagnostic()?.auto_finish()))
        }

        #[cfg(not(feature = "miette"))]
        {
            TarPacker::create(Box::new(encoder?.auto_finish()))
        }
    }

    /// Create a new `.tar.bz2` packer.
    #[cfg(feature = "tar-bz2")]
    pub fn new_bz2(output_file: &Path) -> ArchiveResult<Self> {
        Self::new_bz2_with_level(output_file, 6) // Default in lib
    }

    /// Create a new `.tar.gz` packer with a custom compression level.
    #[cfg(feature = "tar-bz2")]
    pub fn new_bz2_with_level(output_file: &Path, level: u32) -> ArchiveResult<Self> {
        TarPacker::create(Box::new(bzip2::write::BzEncoder::new(
            fs::create_file(output_file)?,
            bzip2::Compression::new(level),
        )))
    }
}

impl ArchivePacker for TarPacker {
    fn add_file(&mut self, name: &str, file: &Path) -> ArchiveResult<()> {
        trace!(source = name, input = ?file, "Packing file");

        self.archive
            .append_file(name, &mut fs::open_file(file)?)
            .map_err(|error| TarError::AddFailure {
                source: file.to_path_buf(),
                error: Box::new(error),
            })?;

        Ok(())
    }

    fn add_dir(&mut self, name: &str, dir: &Path) -> ArchiveResult<()> {
        trace!(source = name, input = ?dir, "Packing directory");

        self.archive
            .append_dir_all(name, dir)
            .map_err(|error| TarError::AddFailure {
                source: dir.to_path_buf(),
                error: Box::new(error),
            })?;

        Ok(())
    }

    #[instrument(name = "pack_tar", skip_all)]
    fn pack(&mut self) -> ArchiveResult<()> {
        trace!("Creating tarball");

        self.archive
            .finish()
            .map_err(|error| TarError::PackFailure {
                error: Box::new(error),
            })?;

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
    pub fn create(output_dir: &Path, reader: Box<dyn Read>) -> ArchiveResult<Self> {
        fs::create_dir_all(output_dir)?;

        Ok(TarUnpacker {
            archive: TarArchive::new(reader),
            output_dir: output_dir.to_path_buf(),
        })
    }

    /// Create a new `.tar` unpacker.
    pub fn new(output_dir: &Path, input_file: &Path) -> ArchiveResult<Self> {
        TarUnpacker::create(output_dir, Box::new(fs::open_file(input_file)?))
    }

    /// Create a new `.tar.gz` unpacker.
    #[cfg(feature = "tar-gz")]
    pub fn new_gz(output_dir: &Path, input_file: &Path) -> ArchiveResult<Self> {
        TarUnpacker::create(
            output_dir,
            Box::new(flate2::read::GzDecoder::new(fs::open_file(input_file)?)),
        )
    }

    /// Create a new `.tar.xz` unpacker.
    #[cfg(feature = "tar-xz")]
    pub fn new_xz(output_dir: &Path, input_file: &Path) -> ArchiveResult<Self> {
        TarUnpacker::create(
            output_dir,
            Box::new(liblzma::read::XzDecoder::new(fs::open_file(input_file)?)),
        )
    }

    /// Create a new `.tar.zstd` unpacker.
    #[cfg(feature = "tar-zstd")]
    pub fn new_zstd(output_dir: &Path, input_file: &Path) -> ArchiveResult<Self> {
        let decoder = zstd::stream::Decoder::new(fs::open_file(input_file)?);

        #[cfg(feature = "miette")]
        {
            use miette::IntoDiagnostic;

            TarUnpacker::create(output_dir, Box::new(decoder.into_diagnostic()?))
        }

        #[cfg(not(feature = "miette"))]
        {
            TarUnpacker::create(output_dir, Box::new(decoder?))
        }
    }

    /// Create a new `.tar.bz2` unpacker.
    #[cfg(feature = "tar-bz2")]
    pub fn new_bz2(output_dir: &Path, input_file: &Path) -> ArchiveResult<Self> {
        TarUnpacker::create(
            output_dir,
            Box::new(bzip2::read::BzDecoder::new(fs::open_file(input_file)?)),
        )
    }
}

impl ArchiveUnpacker for TarUnpacker {
    #[instrument(name = "unpack_tar", skip_all)]
    fn unpack(&mut self, prefix: &str, differ: &mut TreeDiffer) -> ArchiveResult<PathBuf> {
        self.archive.set_overwrite(true);

        trace!(output_dir = ?self.output_dir, "Opening tarball");

        let mut count = 0;

        for entry in self
            .archive
            .entries()
            .map_err(|error| TarError::UnpackFailure {
                error: Box::new(error),
            })?
        {
            let mut entry = entry.map_err(|error| TarError::UnpackFailure {
                error: Box::new(error),
            })?;
            let mut path: PathBuf = entry.path().unwrap().into_owned();

            // Remove the prefix
            if !prefix.is_empty() {
                if let Ok(suffix) = path.strip_prefix(prefix) {
                    path = suffix.to_owned();
                }
            }

            // Unpack the file if different than destination
            let output_path = self.output_dir.join(&path);

            if let Some(parent_dir) = output_path.parent() {
                fs::create_dir_all(parent_dir)?;
            }

            // trace!(source = ?path, "Unpacking file");

            // NOTE: gzip doesn't support seeking, so we can't use the following util then!
            // if differ.should_write_source(entry.size(), &mut entry, &output_path)? {
            entry
                .unpack(&output_path)
                .map_err(|error| TarError::ExtractFailure {
                    source: output_path.clone(),
                    error: Box::new(error),
                })?;
            // }

            differ.untrack_file(&output_path);
            count += 1;
        }

        trace!("Unpacked {} files", count);

        Ok(self.output_dir.clone())
    }
}
