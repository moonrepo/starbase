use crate::codecs::{Finish, ReadState, State, WriteState};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use std::io::{self, Read, Write};

/// The default gzip compression level.
pub const GZ_DEFAULT_LEVEL: u32 = 4;

/// A gzip codec that wraps another stream. The first read commits it to
/// decompressing, while the first write commits it to compressing.
pub struct Gz<T> {
    level: u32,
    state: State<T>,
}

impl<T> Gz<T> {
    /// Create a new codec with the default compression level.
    pub fn new(inner: T) -> Self {
        Self::with_level(inner, GZ_DEFAULT_LEVEL)
    }

    /// Create a new codec with a custom compression level (0-9).
    /// The level only applies when compressing.
    pub fn with_level(inner: T, level: u32) -> Self {
        Gz {
            level,
            state: State::Pending(inner),
        }
    }

    /// Consume the codec and return the wrapped stream. If the codec was
    /// compressing, the gzip epilogue is written first.
    pub fn into_inner(self) -> io::Result<T> {
        self.state.into_inner()
    }
}

impl<T: Read + 'static> ReadState<T> for GzDecoder<T> {
    fn into_inner(self: Box<Self>) -> T {
        (*self).into_inner()
    }
}

impl<T: Read + 'static> Read for Gz<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.state
            .read(buf, |inner| Ok(Box::new(GzDecoder::new(inner))))
    }
}

impl<T: Write + 'static> WriteState<T> for GzEncoder<T> {
    fn finish(self: Box<Self>) -> io::Result<T> {
        (*self).finish()
    }
}

impl<T: Write + 'static> Write for Gz<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let level = self.level;

        self.state.write(buf, |inner| {
            Ok(Box::new(GzEncoder::new(inner, Compression::new(level))))
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        self.state.flush()
    }
}

impl<T: Finish + 'static> Finish for Gz<T> {
    fn finish(&mut self) -> io::Result<()> {
        let level = self.level;

        self.state
            .finish(|inner| Ok(Box::new(GzEncoder::new(inner, Compression::new(level)))))
    }
}
