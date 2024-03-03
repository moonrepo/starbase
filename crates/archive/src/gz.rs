use crate::archive::{ArchivePacker, ArchiveResult, ArchiveUnpacker};
use crate::tree_differ::TreeDiffer;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use starbase_utils::fs;
use std::fs::File;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use tracing::trace;

pub use crate::gz_error::GzError;

/// Applies gzip to a file.
pub struct GzPacker {
    archive: Option<GzEncoder<File>>,
}

impl GzPacker {
    /// Create a new packer with a custom compression level.
    pub fn create(output_file: &Path, compression: Compression) -> ArchiveResult<Self> {
        Ok(GzPacker {
            archive: Some(GzEncoder::new(fs::create_file(output_file)?, compression)),
        })
    }

    /// Create a new `.gz` packer.
    pub fn new(output_file: &Path) -> ArchiveResult<Self> {
        Self::create(output_file, Compression::default())
    }
}

impl ArchivePacker for GzPacker {
    fn add_file(&mut self, _name: &str, file: &Path) -> ArchiveResult<()> {
        self.archive
            .as_mut()
            .unwrap()
            .write_all(&fs::read_file_bytes(file)?)
            .map_err(|error| GzError::AddFailure {
                source: file.to_path_buf(),
                error,
            })?;

        Ok(())
    }

    fn add_dir(&mut self, _name: &str, _dir: &Path) -> ArchiveResult<()> {
        Ok(())
    }

    fn pack(&mut self) -> ArchiveResult<()> {
        trace!("Gzipping file");

        self.archive
            .take()
            .unwrap()
            .finish()
            .map_err(|error| GzError::PackFailure { error })?;

        Ok(())
    }
}

/// Opens a gzipped file.
pub struct GzUnpacker {
    archive: GzDecoder<File>,
    output_dir: PathBuf,
}

impl GzUnpacker {
    /// Create a new `.gz` unpacker.
    pub fn new(output_dir: &Path, input_file: &Path) -> ArchiveResult<Self> {
        fs::create_dir_all(output_dir)?;

        Ok(GzUnpacker {
            archive: GzDecoder::new(fs::open_file(input_file)?),
            output_dir: output_dir.to_path_buf(),
        })
    }
}

impl ArchiveUnpacker for GzUnpacker {
    fn unpack(&mut self, prefix: &str, differ: &mut TreeDiffer) -> ArchiveResult<()> {
        trace!(output_dir = ?self.output_dir, "Opening zip");

        Ok(())
    }
}
