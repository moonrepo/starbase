use crate::archive::{ArchivePacker, ArchiveUnpacker};
use crate::archive_error::ArchiveError;
use crate::codecs::Finish;
use starbase_utils::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use tracing::{instrument, trace};

pub use crate::file_error::FileError;

/// Packs a single file, verbatim, into the provided stream. Compose the
/// stream with a codec to create compressed single-file archives, since
/// a bare codec has no concept of files on its own.
///
/// ```ignore
/// FilePacker::new(Gz::new(fs::create_file(path)?));   // .gz
/// FilePacker::new(Zstd::new(fs::create_file(path)?)); // .zst
/// ```
pub struct FilePacker<W: Write> {
    file_count: usize,
    writer: W,
}

impl<W: Write> FilePacker<W> {
    /// Create a new single-file packer that writes to the provided stream.
    pub fn new(writer: W) -> Self {
        FilePacker {
            file_count: 0,
            writer,
        }
    }
}

impl<W: Write + Finish> ArchivePacker for FilePacker<W> {
    fn add_file(&mut self, name: &str, file: &Path) -> Result<(), ArchiveError> {
        trace!(source = name, input = ?file, "Adding file");

        if self.file_count > 0 {
            return Err(FileError::OneFile.into());
        }

        io::copy(&mut fs::open_file(file)?, &mut self.writer).map_err(|error| {
            FileError::AddFailure {
                source: file.to_path_buf(),
                error: Box::new(error),
            }
        })?;

        self.file_count += 1;

        Ok(())
    }

    fn add_dir(&mut self, _name: &str, _dir: &Path) -> Result<(), ArchiveError> {
        Err(FileError::NoDirs.into())
    }

    #[instrument(name = "pack_file", skip_all)]
    fn pack(mut self) -> Result<(), ArchiveError> {
        trace!("Packing single-file archive");

        // Writes codec epilogues through the entire stream chain.
        self.writer
            .finish()
            .map_err(|error| FileError::PackFailure {
                error: Box::new(error),
            })?;

        Ok(())
    }
}

/// Unpacks the provided stream into a single output file. Compose the
/// stream with a codec to open compressed single-file archives.
///
/// ```ignore
/// FileUnpacker::new(dir.join("file.txt"), Gz::new(fs::open_file(path)?));
/// ```
pub struct FileUnpacker<R: Read> {
    output_file: PathBuf,
    reader: R,
}

impl<R: Read> FileUnpacker<R> {
    /// Create a new single-file unpacker that reads from the provided
    /// stream into the output file.
    pub fn new(output_file: impl AsRef<Path>, reader: R) -> Self {
        FileUnpacker {
            output_file: output_file.as_ref().to_path_buf(),
            reader,
        }
    }
}

impl<R: Read> ArchiveUnpacker for FileUnpacker<R> {
    #[instrument(name = "unpack_file", skip_all)]
    fn unpack(mut self, _prefix: &str) -> Result<PathBuf, ArchiveError> {
        trace!(output_file = ?self.output_file, "Unpacking single-file archive");

        let mut output = fs::create_file(&self.output_file)?;

        io::copy(&mut self.reader, &mut output).map_err(|error| FileError::UnpackFailure {
            error: Box::new(error),
        })?;

        Ok(self.output_file)
    }
}
