use crate::codecs::{Finish, ReadState, State, WriteState};
use liblzma::read::XzDecoder;
use liblzma::write::XzEncoder;
use std::io::{self, Read, Write};

/// The default xz compression level.
pub const XZ_DEFAULT_LEVEL: u32 = 4;

/// An xz codec that wraps another stream. The first read commits it to
/// decompressing, while the first write commits it to compressing.
pub struct Xz<T> {
    level: u32,
    state: State<T>,
}

impl<T> Xz<T> {
    /// Create a new codec with the default compression level.
    pub fn new(inner: T) -> Self {
        Self::with_level(inner, XZ_DEFAULT_LEVEL)
    }

    /// Create a new codec with a custom compression level (0-9).
    /// The level only applies when compressing.
    pub fn with_level(inner: T, level: u32) -> Self {
        Xz {
            level,
            state: State::Pending(inner),
        }
    }

    /// Consume the codec and return the wrapped stream. If the codec was
    /// compressing, the xz epilogue is written first.
    pub fn into_inner(self) -> io::Result<T> {
        self.state.into_inner()
    }
}

impl<T: Read + 'static> ReadState<T> for XzDecoder<T> {
    fn into_inner(self: Box<Self>) -> T {
        (*self).into_inner()
    }
}

impl<T: Read + 'static> Read for Xz<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.state
            .read(buf, |inner| Ok(Box::new(XzDecoder::new(inner))))
    }
}

impl<T: Write + 'static> WriteState<T> for XzEncoder<T> {
    fn finish(self: Box<Self>) -> io::Result<T> {
        (*self).finish()
    }
}

impl<T: Write + 'static> Write for Xz<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let level = self.level;

        self.state
            .write(buf, |inner| Ok(Box::new(XzEncoder::new(inner, level))))
    }

    fn flush(&mut self) -> io::Result<()> {
        self.state.flush()
    }
}

impl<T: Finish + 'static> Finish for Xz<T> {
    fn finish(&mut self) -> io::Result<()> {
        let level = self.level;

        self.state
            .finish(|inner| Ok(Box::new(XzEncoder::new(inner, level))))
    }
}
