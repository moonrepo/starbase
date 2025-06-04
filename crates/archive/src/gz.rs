use crate::archive::{ArchivePacker, ArchiveUnpacker};
use crate::archive_error::ArchiveError;
pub use crate::gz_error::GzError;
use crate::tree_differ::TreeDiffer;
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use starbase_utils::fs;
use std::fs::File;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use tracing::{instrument, trace};

/// Applies gzip to a single file.
pub struct GzPacker {
    archive: Option<GzEncoder<File>>,
    file_count: usize,
}

impl GzPacker {
    /// Create a new packer with a custom compression level.
    pub fn create(output_file: &Path, compression: Compression) -> Result<Self, ArchiveError> {
        Ok(GzPacker {
            archive: Some(GzEncoder::new(fs::create_file(output_file)?, compression)),
            file_count: 0,
        })
    }

    /// Create a new `.gz` packer.
    pub fn new(output_file: &Path) -> Result<Self, ArchiveError> {
        Self::create(output_file, Compression::default())
    }
}

impl ArchivePacker for GzPacker {
    fn add_file(&mut self, _name: &str, file: &Path) -> Result<(), ArchiveError> {
        if self.file_count > 0 {
            return Err(GzError::OneFile.into());
        }

        self.archive
            .as_mut()
            .unwrap()
            .write_all(&fs::read_file_bytes(file)?)
            .map_err(|error| GzError::AddFailure {
                source: file.to_path_buf(),
                error: Box::new(error),
            })?;

        self.file_count += 1;

        Ok(())
    }

    fn add_dir(&mut self, _name: &str, _dir: &Path) -> Result<(), ArchiveError> {
        Err(ArchiveError::Gz(Box::new(GzError::NoDirs)))
    }

    #[instrument(name = "pack_gz", skip_all)]
    fn pack(&mut self) -> Result<(), ArchiveError> {
        trace!("Gzipping file");

        self.archive
            .take()
            .unwrap()
            .finish()
            .map_err(|error| GzError::PackFailure {
                error: Box::new(error),
            })?;

        Ok(())
    }
}

/// Opens a gzipped file.
pub struct GzUnpacker {
    archive: GzDecoder<File>,
    file_name: String,
    output_dir: PathBuf,
}

impl GzUnpacker {
    /// Create a new `.gz` unpacker.
    pub fn new(output_dir: &Path, input_file: &Path) -> Result<Self, ArchiveError> {
        fs::create_dir_all(output_dir)?;

        Ok(GzUnpacker {
            archive: GzDecoder::new(fs::open_file(input_file)?),
            file_name: fs::file_name(input_file).replace(".gz", ""),
            output_dir: output_dir.to_path_buf(),
        })
    }
}

impl ArchiveUnpacker for GzUnpacker {
    #[instrument(name = "unpack_gz", skip_all)]
    fn unpack(&mut self, _prefix: &str, _differ: &mut TreeDiffer) -> Result<PathBuf, ArchiveError> {
        trace!(output_dir = ?self.output_dir, "Ungzipping file");

        let mut bytes = vec![];

        self.archive
            .read_to_end(&mut bytes)
            .map_err(|error| GzError::UnpackFailure {
                error: Box::new(error),
            })?;

        let out_file = self.output_dir.join(&self.file_name);

        fs::write_file(&out_file, bytes)?;

        Ok(out_file)
    }
}
