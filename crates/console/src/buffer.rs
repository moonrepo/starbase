use crate::stream::ConsoleStreamType;
use parking_lot::Mutex;
use std::io::{self, Write};
use std::sync::{Arc, mpsc};
use std::thread::sleep;
use std::time::Duration;

pub struct ConsoleBuffer {
    buffer: Arc<Mutex<Vec<u8>>>,
    stream: ConsoleStreamType,
}

impl ConsoleBuffer {
    pub fn new(buffer: Arc<Mutex<Vec<u8>>>, stream: ConsoleStreamType) -> Self {
        Self { buffer, stream }
    }
}

impl Write for ConsoleBuffer {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buffer.lock().extend_from_slice(data);

        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        flush(&mut self.buffer.lock(), self.stream)
    }
}

pub fn flush(buffer: &mut Vec<u8>, stream: ConsoleStreamType) -> io::Result<()> {
    if buffer.is_empty() {
        return Ok(());
    }

    match stream {
        ConsoleStreamType::Stderr => flush_into(buffer, &mut io::stderr().lock()),
        ConsoleStreamType::Stdout => flush_into(buffer, &mut io::stdout().lock()),
    }
}

pub(crate) fn flush_into<W: Write>(buffer: &mut Vec<u8>, w: &mut W) -> io::Result<()> {
    let mut written = 0;

    while written < buffer.len() {
        match w.write(&buffer[written..]) {
            Ok(0) => {
                if written > 0 {
                    buffer.drain(..written);
                }

                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "failed to write whole buffer",
                ));
            }
            Ok(n) => {
                written += n;
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                break;
            }
            Err(e) => {
                if written > 0 {
                    buffer.drain(..written);
                }

                return Err(e);
            }
        }
    }

    if written > 0 {
        buffer.drain(..written);
    }

    Ok(())
}

pub fn flush_on_loop(
    buffer: Arc<Mutex<Vec<u8>>>,
    stream: ConsoleStreamType,
    receiver: mpsc::Receiver<bool>,
) {
    loop {
        sleep(Duration::from_millis(100));

        let _ = flush(&mut buffer.lock(), stream);

        // Has the thread been closed?
        match receiver.try_recv() {
            Ok(true) | Err(mpsc::TryRecvError::Disconnected) => {
                // Flush once more to capture anything written between the flush
                // above and this shutdown signal, then exit.
                let _ = flush(&mut buffer.lock(), stream);
                break;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// Programmable mock writer: each call to `write` consumes one queued
    /// response. `Ok(usize)` is mapped to "wrote that many bytes from the
    /// front of the input slice".
    enum Step {
        WroteAll,
        WroteN(usize),
        WroteZero,
        WouldBlock,
        Interrupted,
        OtherErr(io::ErrorKind),
    }

    struct MockWriter {
        steps: VecDeque<Step>,
        sink: Vec<u8>,
        always_wouldblock: bool,
    }

    impl MockWriter {
        fn new(steps: Vec<Step>) -> Self {
            Self {
                steps: steps.into(),
                sink: Vec::new(),
                always_wouldblock: false,
            }
        }

        fn always_wouldblock() -> Self {
            Self {
                steps: VecDeque::new(),
                sink: Vec::new(),
                always_wouldblock: true,
            }
        }
    }

    impl Write for MockWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if self.always_wouldblock {
                return Err(io::Error::from(io::ErrorKind::WouldBlock));
            }
            match self
                .steps
                .pop_front()
                .expect("MockWriter ran out of programmed steps")
            {
                Step::WroteAll => {
                    self.sink.extend_from_slice(buf);
                    Ok(buf.len())
                }
                Step::WroteN(n) => {
                    let n = n.min(buf.len());
                    self.sink.extend_from_slice(&buf[..n]);
                    Ok(n)
                }
                Step::WroteZero => Ok(0),
                Step::WouldBlock => Err(io::Error::from(io::ErrorKind::WouldBlock)),
                Step::Interrupted => Err(io::Error::from(io::ErrorKind::Interrupted)),
                Step::OtherErr(kind) => Err(io::Error::from(kind)),
            }
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn interrupted_is_retried_inline() {
        let mut w = MockWriter::new(vec![Step::Interrupted, Step::WroteAll]);
        let mut buffer = b"hi".to_vec();

        flush_into(&mut buffer, &mut w).unwrap();

        assert_eq!(w.sink, b"hi");
        assert!(buffer.is_empty());
    }

    #[test]
    fn wouldblock_leaves_remainder_in_buffer() {
        let mut w = MockWriter::new(vec![Step::WroteN(2), Step::WouldBlock]);
        let mut buffer = b"hello".to_vec();

        flush_into(&mut buffer, &mut w).unwrap();

        assert_eq!(w.sink, b"he");
        assert_eq!(buffer, b"llo");
    }

    #[test]
    fn pure_wouldblock_leaves_buffer_untouched() {
        let mut w = MockWriter::always_wouldblock();
        let mut buffer = b"hello".to_vec();

        flush_into(&mut buffer, &mut w).unwrap();

        assert!(w.sink.is_empty());
        assert_eq!(buffer, b"hello");
    }

    #[test]
    fn successful_write_drains_buffer() {
        let mut w = MockWriter::new(vec![Step::WroteAll]);
        let mut buffer = b"hello".to_vec();

        flush_into(&mut buffer, &mut w).unwrap();

        assert_eq!(w.sink, b"hello");
        assert!(buffer.is_empty());
    }

    // Non-WouldBlock/Interrupted errors are not transient — propagate
    // immediately. BrokenPipe in particular
    // is the #2465 SIGPIPE variant; moon's top-level handler decides how
    // to render it.
    #[test]
    fn other_errors_propagate_immediately() {
        let mut w = MockWriter::new(vec![
            Step::WroteN(2),
            Step::OtherErr(io::ErrorKind::BrokenPipe),
        ]);
        let mut buffer = b"hello".to_vec();

        let err = flush_into(&mut buffer, &mut w).unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
        assert_eq!(w.sink, b"he");
        assert_eq!(buffer, b"llo");
    }

    // Ok(0) from a writer with non-empty input means it can no longer
    // make progress; mirror std::io::Write::write_all's contract.
    #[test]
    fn write_zero_returns_writezero() {
        let mut w = MockWriter::new(vec![Step::WroteZero]);
        let mut buffer = b"x".to_vec();

        let err = flush_into(&mut buffer, &mut w).unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::WriteZero);
        assert_eq!(buffer, b"x");
    }

    #[test]
    fn empty_input_is_noop() {
        let mut w = MockWriter::new(vec![]);
        let mut buffer = Vec::new();

        flush_into(&mut buffer, &mut w).unwrap();

        assert!(buffer.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn real_nonblocking_pipe_full_returns_ok_with_remainder() {
        use std::os::fd::FromRawFd;

        let mut fds = [0_i32; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        let read_fd = fds[0];
        let write_fd = fds[1];

        // Make the write side non-blocking: writes that would normally
        // park the kernel pipe full instead return EAGAIN.
        unsafe {
            let flags = libc::fcntl(write_fd, libc::F_GETFL, 0);
            assert!(flags >= 0);
            assert_eq!(
                libc::fcntl(write_fd, libc::F_SETFL, flags | libc::O_NONBLOCK),
                0
            );
        }

        let mut writer = unsafe { std::fs::File::from_raw_fd(write_fd) };
        let mut fill = vec![b'x'; 1024 * 1024];
        flush_into(&mut fill, &mut writer).unwrap();
        assert!(!fill.is_empty(), "pipe should fill before 1 MiB is written");

        let original = vec![b'y'; 1024 * 1024];
        let mut buffer = original.clone();

        flush_into(&mut buffer, &mut writer).unwrap();

        assert_eq!(buffer, original);

        drop(writer);
        unsafe { libc::close(read_fd) };
    }

    #[cfg(unix)]
    #[test]
    fn real_nonblocking_pipe_repeated_flushes_drain_into_slow_reader() {
        use std::os::fd::FromRawFd;
        use std::thread;

        let mut fds = [0_i32; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        let read_fd = fds[0];
        let write_fd = fds[1];

        unsafe {
            let flags = libc::fcntl(write_fd, libc::F_GETFL, 0);
            assert_eq!(
                libc::fcntl(write_fd, libc::F_SETFL, flags | libc::O_NONBLOCK),
                0
            );
        }

        let payload = vec![b'x'; 1024 * 1024];
        let expected = payload.clone();
        let reader = thread::spawn(move || {
            thread::sleep(Duration::from_millis(150));

            let mut received = Vec::new();
            let mut buf = [0_u8; 8192];

            loop {
                let n = unsafe { libc::read(read_fd, buf.as_mut_ptr() as *mut _, buf.len()) };

                if n <= 0 {
                    break;
                }

                received.extend_from_slice(&buf[..n as usize]);
                thread::sleep(Duration::from_millis(10));
            }

            unsafe { libc::close(read_fd) };
            received
        });

        let mut writer = unsafe { std::fs::File::from_raw_fd(write_fd) };
        let mut buffer = payload;

        for _ in 0..200 {
            flush_into(&mut buffer, &mut writer).unwrap();

            if buffer.is_empty() {
                break;
            }

            thread::sleep(Duration::from_millis(100));
        }

        assert!(buffer.is_empty(), "buffer should eventually drain");

        drop(writer);

        let received = reader.join().expect("reader panicked");
        assert_eq!(received, expected);
    }
}
