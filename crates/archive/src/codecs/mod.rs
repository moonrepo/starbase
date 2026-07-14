//! Compression codecs that wrap read/write streams.
//!
//! A codec is a pure byte-stream transformer with no concept of files or
//! directories. Codecs compose with formats (and each other) by wrapping
//! the stream they read from or write to:
//!
//! ```ignore
//! TarPacker::new(Gz::new(fs::create_file(path)?))     // pack a .tar.gz
//! TarUnpacker::new(dir, Gz::new(fs::open_file(path)?)) // unpack a .tar.gz
//! ```
//!
//! Each codec commits to a direction on first use: the first [`std::io::Read`]
//! call turns it into a decompressor, the first [`std::io::Write`] call into
//! a compressor. Using both directions on the same instance is an error.

#[cfg(feature = "bz2")]
mod bz2;
#[cfg(feature = "gz")]
mod gz;
mod state;
#[cfg(feature = "xz")]
mod xz;
#[cfg(feature = "z")]
mod z;
#[cfg(feature = "zstd")]
mod zstd;

#[cfg(feature = "bz2")]
pub use bz2::Bz2;
#[cfg(feature = "gz")]
pub use gz::Gz;
pub use state::*;
#[cfg(feature = "xz")]
pub use xz::Xz;
#[cfg(feature = "z")]
pub use z::Z;
#[cfg(feature = "zstd")]
pub use zstd::Zstd;

use std::fs::File;
use std::io::{self, BufWriter, Cursor, Write};

/// A writable stream that must be explicitly finalized, so that any
/// trailing bytes (compression epilogues, checksums, etc.) are written
/// and flushed through the entire stream chain.
///
/// Codecs implement this by writing their epilogue and then finishing the
/// stream they wrap, so a call on the outermost stream cascades all the
/// way down. Plain sinks like [`File`] and [`Vec`] simply flush.
///
/// Implement this trait (typically by delegating to [`Write::flush`]) to
/// use a custom writer with the packers in this crate.
pub trait Finish: Write {
    /// Write any trailing bytes and flush the stream chain.
    fn finish(&mut self) -> io::Result<()>;
}

impl Finish for File {
    fn finish(&mut self) -> io::Result<()> {
        self.flush()
    }
}

impl Finish for Vec<u8> {
    fn finish(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<T> Finish for Cursor<T>
where
    Cursor<T>: Write,
{
    fn finish(&mut self) -> io::Result<()> {
        self.flush()
    }
}

impl<W: Finish> Finish for BufWriter<W> {
    fn finish(&mut self) -> io::Result<()> {
        self.flush()?;
        self.get_mut().finish()
    }
}

impl<F: Finish + ?Sized> Finish for Box<F> {
    fn finish(&mut self) -> io::Result<()> {
        (**self).finish()
    }
}

impl<F: Finish + ?Sized> Finish for &mut F {
    fn finish(&mut self) -> io::Result<()> {
        (**self).finish()
    }
}
