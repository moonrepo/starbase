use crate::codecs::Finish;
use std::io::{self, Read, Write};
use std::mem;

/// Read-direction codec state: a decompressor that owns the wrapped stream.
pub trait ReadState<T>: Read {
    /// Return the wrapped stream, discarding decoder state.
    fn into_inner(self: Box<Self>) -> T;
}

/// Write-direction codec state: a compressor that owns the wrapped stream.
pub trait WriteState<T>: Write {
    /// Write the codec epilogue and return the wrapped stream.
    fn finish(self: Box<Self>) -> io::Result<T>;
}

/// Lazily-directed codec state. A codec starts as [`State::Pending`] and
/// commits to reading (decompressing) or writing (compressing) on first use.
pub enum State<T> {
    /// Direction not chosen yet.
    Pending(T),
    /// Committed to decompressing reads.
    Reading(Box<dyn ReadState<T>>),
    /// Committed to compressing writes.
    Writing(Box<dyn WriteState<T>>),
    /// Epilogue written; the wrapped stream has been recovered.
    Finished(T),
    /// A state transition failed and the wrapped stream was lost.
    Poisoned,
}

impl<T> State<T> {
    fn take(&mut self) -> State<T> {
        mem::replace(self, State::Poisoned)
    }

    /// Read from the decompressor, committing the pending stream to the
    /// read direction on first use.
    pub fn read(
        &mut self,
        buf: &mut [u8],
        init: impl FnOnce(T) -> io::Result<Box<dyn ReadState<T>>>,
    ) -> io::Result<usize> {
        if matches!(self, State::Pending(_)) {
            let State::Pending(inner) = self.take() else {
                unreachable!()
            };

            *self = State::Reading(init(inner)?);
        }

        match self {
            State::Reading(decoder) => decoder.read(buf),
            State::Writing(_) => Err(direction_error("read from", "compressing")),
            State::Finished(_) => Err(finished_error("read from")),
            State::Poisoned => Err(poisoned_error()),
            State::Pending(_) => unreachable!(),
        }
    }

    /// Write to the compressor, committing the pending stream to the
    /// write direction on first use.
    pub fn write(
        &mut self,
        buf: &[u8],
        init: impl FnOnce(T) -> io::Result<Box<dyn WriteState<T>>>,
    ) -> io::Result<usize> {
        if matches!(self, State::Pending(_)) {
            let State::Pending(inner) = self.take() else {
                unreachable!()
            };

            *self = State::Writing(init(inner)?);
        }

        match self {
            State::Writing(encoder) => encoder.write(buf),
            State::Reading(_) => Err(direction_error("write to", "decompressing")),
            State::Finished(_) => Err(finished_error("write to")),
            State::Poisoned => Err(poisoned_error()),
            State::Pending(_) => unreachable!(),
        }
    }

    pub fn flush(&mut self) -> io::Result<()> {
        match self {
            State::Writing(encoder) => encoder.flush(),
            // Nothing has been written yet, or trailing bytes were
            // already flushed by `finish`.
            State::Pending(_) | State::Finished(_) => Ok(()),
            State::Reading(_) => Err(direction_error("flush", "decompressing")),
            State::Poisoned => Err(poisoned_error()),
        }
    }

    /// Write the codec epilogue and finish the wrapped stream. Idempotent
    /// once finished. If nothing was written yet, the codec still commits
    /// to the write direction so the output is a valid, empty stream.
    pub fn finish(
        &mut self,
        init: impl FnOnce(T) -> io::Result<Box<dyn WriteState<T>>>,
    ) -> io::Result<()>
    where
        T: Finish,
    {
        if matches!(self, State::Pending(_)) {
            let State::Pending(inner) = self.take() else {
                unreachable!()
            };

            *self = State::Writing(init(inner)?);
        }

        match self.take() {
            State::Writing(encoder) => {
                let mut inner = encoder.finish()?;
                let result = inner.finish();

                *self = State::Finished(inner);

                result
            }
            state @ State::Finished(_) => {
                *self = state;

                Ok(())
            }
            state @ State::Reading(_) => {
                *self = state;

                Err(direction_error("finish", "decompressing"))
            }
            State::Poisoned => Err(poisoned_error()),
            State::Pending(_) => unreachable!(),
        }
    }

    /// Consume the state and return the wrapped stream. If committed to
    /// the write direction, the codec epilogue is written first (but the
    /// wrapped stream is *not* finished, since the caller now owns it).
    pub fn into_inner(self) -> io::Result<T> {
        match self {
            State::Pending(inner) | State::Finished(inner) => Ok(inner),
            State::Reading(decoder) => Ok(decoder.into_inner()),
            State::Writing(encoder) => encoder.finish(),
            State::Poisoned => Err(poisoned_error()),
        }
    }
}

fn direction_error(operation: &str, direction: &str) -> io::Error {
    io::Error::other(format!("cannot {operation} a codec that is {direction}"))
}

fn finished_error(operation: &str) -> io::Error {
    io::Error::other(format!("cannot {operation} a finished codec"))
}

fn poisoned_error() -> io::Error {
    io::Error::other("codec is poisoned from a previous failure")
}
