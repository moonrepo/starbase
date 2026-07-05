use crate::codecs::{Finish, ReadState, State, WriteState};
use std::io::{self, BufReader, Read, Write};
use zstd::stream::read::Decoder;
use zstd::stream::write::Encoder;

/// The default zstd compression level.
pub const ZSTD_DEFAULT_LEVEL: i32 = 3;

/// A zstd codec that wraps another stream. The first read commits it to
/// decompressing, while the first write commits it to compressing.
pub struct Zstd<T> {
    level: i32,
    state: State<T>,
}

impl<T> Zstd<T> {
    /// Create a new codec with the default compression level.
    pub fn new(inner: T) -> Self {
        Self::with_level(inner, ZSTD_DEFAULT_LEVEL)
    }

    /// Create a new codec with a custom compression level (1-22).
    /// The level only applies when compressing.
    pub fn with_level(inner: T, level: i32) -> Self {
        Zstd {
            level,
            state: State::Pending(inner),
        }
    }

    /// Consume the codec and return the wrapped stream. If the codec was
    /// compressing, the zstd epilogue is written first. If it was
    /// decompressing, any bytes buffered past the compressed frame are lost.
    pub fn into_inner(self) -> io::Result<T> {
        self.state.into_inner()
    }
}

impl<T: Read + 'static> ReadState<T> for Decoder<'static, BufReader<T>> {
    fn into_inner(self: Box<Self>) -> T {
        (*self).finish().into_inner()
    }
}

impl<T: Read + 'static> Read for Zstd<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.state
            .read(buf, |inner| Ok(Box::new(Decoder::new(inner)?)))
    }
}

impl<T: Write + 'static> WriteState<T> for Encoder<'static, T> {
    fn finish(self: Box<Self>) -> io::Result<T> {
        (*self).finish()
    }
}

impl<T: Write + 'static> Write for Zstd<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let level = self.level;

        self.state
            .write(buf, |inner| Ok(Box::new(Encoder::new(inner, level)?)))
    }

    fn flush(&mut self) -> io::Result<()> {
        self.state.flush()
    }
}

impl<T: Finish + 'static> Finish for Zstd<T> {
    fn finish(&mut self) -> io::Result<()> {
        let level = self.level;

        self.state
            .finish(|inner| Ok(Box::new(Encoder::new(inner, level)?)))
    }
}
