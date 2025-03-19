use crate::buffer::*;
use crate::console_error::ConsoleError;
use parking_lot::Mutex;
use std::fmt;
use std::io::{self, IsTerminal};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{JoinHandle, spawn};
use tracing::trace;

#[derive(Clone, Copy, Debug)]
pub enum ConsoleStreamType {
    Stderr,
    Stdout,
}

pub struct ConsoleStream {
    buffer: Arc<Mutex<Vec<u8>>>,
    channel: Option<mpsc::Sender<bool>>,
    stream: ConsoleStreamType,

    pub(crate) handle: Option<JoinHandle<()>>,
    pub(crate) quiet: Option<Arc<AtomicBool>>,
    pub(crate) test_mode: bool,
}

impl ConsoleStream {
    fn internal_new(stream: ConsoleStreamType, with_handle: bool) -> Self {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let buffer_clone = Arc::clone(&buffer);
        let (tx, rx) = mpsc::channel();

        // Every 100ms, flush the buffer
        let handle = if with_handle {
            Some(spawn(move || flush_on_loop(buffer_clone, stream, rx)))
        } else {
            None
        };

        Self {
            buffer,
            channel: Some(tx),
            handle,
            stream,
            quiet: None,
            test_mode: false,
        }
    }

    pub fn new(stream: ConsoleStreamType) -> Self {
        Self::internal_new(stream, true)
    }

    pub fn new_testing(stream: ConsoleStreamType) -> Self {
        let mut console = Self::internal_new(stream, false);
        console.test_mode = true;
        console
    }

    pub fn empty(stream: ConsoleStreamType) -> Self {
        Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
            channel: None,
            stream,
            handle: None,
            quiet: None,
            test_mode: false,
        }
    }

    pub fn is_quiet(&self) -> bool {
        self.quiet
            .as_ref()
            .is_some_and(|quiet| quiet.load(Ordering::Relaxed))
    }

    pub fn is_terminal(&self) -> bool {
        match self.stream {
            ConsoleStreamType::Stderr => io::stderr().is_terminal(),
            ConsoleStreamType::Stdout => io::stdout().is_terminal(),
        }
    }

    pub fn buffer(&self) -> ConsoleBuffer {
        ConsoleBuffer::new(self.buffer.clone(), self.stream)
    }

    pub fn close(&self) -> Result<(), ConsoleError> {
        trace!(
            "Closing {} stream",
            match self.stream {
                ConsoleStreamType::Stderr => "stderr",
                ConsoleStreamType::Stdout => "stdout",
            }
        );

        self.flush()?;

        // Send the closed message
        if let Some(channel) = &self.channel {
            let _ = channel.send(true);
        }

        Ok(())
    }

    pub fn flush(&self) -> Result<(), ConsoleError> {
        flush(&mut self.buffer.lock(), self.stream).map_err(|error| ConsoleError::FlushFailed {
            error: Box::new(error),
        })?;

        Ok(())
    }

    pub fn write_raw<F: FnMut(&mut Vec<u8>) -> io::Result<()>>(
        &self,
        mut op: F,
    ) -> Result<(), ConsoleError> {
        let handle_error = |error: io::Error| ConsoleError::WriteFailed {
            error: Box::new(error),
        };

        // When testing just flush immediately
        if self.test_mode {
            let mut buffer = Vec::new();

            op(&mut buffer).map_err(handle_error)?;

            flush(&mut buffer, self.stream).map_err(handle_error)?;
        }
        // Otherwise just write to the buffer and flush
        // when its length grows too large
        else {
            let mut buffer = self.buffer.lock();

            op(&mut buffer).map_err(handle_error)?;

            if buffer.len() >= 1024 {
                flush(&mut buffer, self.stream).map_err(handle_error)?;
            }
        }

        Ok(())
    }

    pub fn write<T: AsRef<[u8]>>(&self, data: T) -> Result<(), ConsoleError> {
        let data = data.as_ref();

        if data.is_empty() {
            return Ok(());
        }

        self.write_raw(|buffer| {
            buffer.extend_from_slice(data);
            Ok(())
        })
    }

    pub fn write_line<T: AsRef<[u8]>>(&self, data: T) -> Result<(), ConsoleError> {
        let data = data.as_ref();

        self.write_raw(|buffer| {
            if !data.is_empty() {
                buffer.extend_from_slice(data);
            }

            buffer.push(b'\n');
            Ok(())
        })
    }

    pub fn write_line_with_prefix<T: AsRef<str>>(
        &self,
        data: T,
        prefix: &str,
    ) -> Result<(), ConsoleError> {
        let data = data.as_ref();
        let lines = data
            .lines()
            .map(|line| format!("{prefix}{line}"))
            .collect::<Vec<_>>()
            .join("\n");

        self.write_line(lines)
    }

    pub fn write_newline(&self) -> Result<(), ConsoleError> {
        self.write_line("")
    }
}

impl Clone for ConsoleStream {
    fn clone(&self) -> Self {
        Self {
            buffer: Arc::clone(&self.buffer),
            stream: self.stream,
            quiet: self.quiet.clone(),
            test_mode: self.test_mode,
            // Ignore for clones
            channel: None,
            handle: None,
        }
    }
}

impl fmt::Debug for ConsoleStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConsoleStream")
            .field("buffer", &self.buffer)
            .field("stream", &self.stream)
            .field("quiet", &self.quiet)
            .field("test_mode", &self.test_mode)
            .finish()
    }
}
