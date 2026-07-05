use crate::codecs::{Finish, ReadState, State, WriteState};
use bzip2::Compression;
use bzip2::read::BzDecoder;
use bzip2::write::BzEncoder;
use std::io::{self, Read, Write};

/// The default bzip2 compression level.
pub const BZ2_DEFAULT_LEVEL: u32 = 6;

/// A bzip2 codec that wraps another stream. The first read commits it to
/// decompressing, while the first write commits it to compressing.
pub struct Bz2<T> {
    level: u32,
    state: State<T>,
}

impl<T> Bz2<T> {
    /// Create a new codec with the default compression level.
    pub fn new(inner: T) -> Self {
        Self::with_level(inner, BZ2_DEFAULT_LEVEL)
    }

    /// Create a new codec with a custom compression level (1-9).
    /// The level only applies when compressing.
    pub fn with_level(inner: T, level: u32) -> Self {
        Bz2 {
            level,
            state: State::Pending(inner),
        }
    }

    /// Consume the codec and return the wrapped stream. If the codec was
    /// compressing, the bzip2 epilogue is written first.
    pub fn into_inner(self) -> io::Result<T> {
        self.state.into_inner()
    }
}

impl<T: Read + 'static> ReadState<T> for BzDecoder<T> {
    fn into_inner(self: Box<Self>) -> T {
        (*self).into_inner()
    }
}

impl<T: Read + 'static> Read for Bz2<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.state
            .read(buf, |inner| Ok(Box::new(BzDecoder::new(inner))))
    }
}

impl<T: Write + 'static> WriteState<T> for BzEncoder<T> {
    fn finish(self: Box<Self>) -> io::Result<T> {
        (*self).finish()
    }
}

impl<T: Write + 'static> Write for Bz2<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let level = self.level;

        self.state.write(buf, |inner| {
            Ok(Box::new(BzEncoder::new(inner, Compression::new(level))))
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        self.state.flush()
    }
}

impl<T: Finish + 'static> Finish for Bz2<T> {
    fn finish(&mut self) -> io::Result<()> {
        let level = self.level;

        self.state
            .finish(|inner| Ok(Box::new(BzEncoder::new(inner, Compression::new(level)))))
    }
}
