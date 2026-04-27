use crate::stream::ConsoleStreamType;
use parking_lot::Mutex;
use std::io::{self, Write};
use std::mem;
use std::sync::{Arc, mpsc};
use std::thread::sleep;
use std::time::{Duration, Instant};

// Upper bound on how long a single flush will tolerate sustained EAGAIN
// before surfacing it as an error. Beyond this we assume the consumer is
// genuinely hung rather than transiently slow.
const FLUSH_WOULDBLOCK_DEADLINE: Duration = Duration::from_secs(30);
const WOULDBLOCK_BACKOFF_MIN: Duration = Duration::from_millis(1);
const WOULDBLOCK_BACKOFF_MAX: Duration = Duration::from_millis(100);

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

    let data = mem::take(buffer);

    match stream {
        ConsoleStreamType::Stderr => {
            write_all_retrying(&mut io::stderr().lock(), &data, FLUSH_WOULDBLOCK_DEADLINE)
        }
        ConsoleStreamType::Stdout => {
            write_all_retrying(&mut io::stdout().lock(), &data, FLUSH_WOULDBLOCK_DEADLINE)
        }
    }
}

// Like Write::write_all, but tolerates `WouldBlock` (EAGAIN/EWOULDBLOCK) by
// sleeping and retrying with exponential backoff. Inherited non-blocking
// stdio (notably from GitHub Actions' log forwarder pipes) is otherwise
// fatal here, even though the underlying condition is transient.
//
// `Interrupted` is also retried; std's `write_all` would have done that for
// us, but once we drop `write_all` we own the responsibility.
pub(crate) fn write_all_retrying<W: Write>(
    w: &mut W,
    mut data: &[u8],
    deadline: Duration,
) -> io::Result<()> {
    let until = Instant::now() + deadline;
    let mut backoff = WOULDBLOCK_BACKOFF_MIN;

    while !data.is_empty() {
        match w.write(data) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "failed to write whole buffer",
                ));
            }
            Ok(n) => {
                data = &data[n..];
                backoff = WOULDBLOCK_BACKOFF_MIN;
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                if Instant::now() >= until {
                    return Err(e);
                }
                sleep(backoff);
                backoff = (backoff * 2).min(WOULDBLOCK_BACKOFF_MAX);
            }
            Err(e) => return Err(e),
        }
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

    // Regression for moon #2465 (EAGAIN variant): a non-blocking writer
    // that returns WouldBlock on the first call must be retried, not
    // surfaced as a fatal error.
    #[test]
    fn retries_through_wouldblock() {
        let mut w = MockWriter::new(vec![Step::WouldBlock, Step::WouldBlock, Step::WroteAll]);
        write_all_retrying(&mut w, b"hello", Duration::from_secs(5)).unwrap();
        assert_eq!(w.sink, b"hello");
        assert!(w.steps.is_empty());
    }

    #[test]
    fn retries_through_interrupted() {
        let mut w = MockWriter::new(vec![Step::Interrupted, Step::WroteAll]);
        write_all_retrying(&mut w, b"hi", Duration::from_secs(5)).unwrap();
        assert_eq!(w.sink, b"hi");
    }

    // Real-world EAGAIN case: writer accepts a partial chunk, then EAGAIN,
    // then the rest. Backoff must reset between successful writes so
    // sustained slow drains don't accumulate latency.
    #[test]
    fn handles_partial_writes_with_wouldblock_between() {
        let mut w = MockWriter::new(vec![
            Step::WroteN(2),
            Step::WouldBlock,
            Step::WroteN(2),
            Step::WroteAll,
        ]);
        write_all_retrying(&mut w, b"abcdefg", Duration::from_secs(5)).unwrap();
        assert_eq!(w.sink, b"abcdefg");
    }

    // After the deadline, persistent WouldBlock must surface as an
    // io::Error so callers can detect a genuinely hung consumer rather
    // than retry forever.
    #[test]
    fn wouldblock_returns_error_after_deadline() {
        let mut w = MockWriter::always_wouldblock();
        let err = write_all_retrying(&mut w, b"x", Duration::from_millis(50)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::WouldBlock);
    }

    // Non-WouldBlock/Interrupted errors are not transient — propagate
    // immediately without burning the deadline. BrokenPipe in particular
    // is the #2465 SIGPIPE variant; moon's top-level handler decides how
    // to render it.
    #[test]
    fn other_errors_propagate_immediately() {
        let mut w = MockWriter::new(vec![Step::OtherErr(io::ErrorKind::BrokenPipe)]);
        let err = write_all_retrying(&mut w, b"x", Duration::from_secs(60)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    }

    // Ok(0) from a writer with non-empty input means it can no longer
    // make progress; mirror std::io::Write::write_all's contract.
    #[test]
    fn write_zero_returns_writezero() {
        let mut w = MockWriter::new(vec![Step::WroteZero]);
        let err = write_all_retrying(&mut w, b"x", Duration::from_secs(5)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::WriteZero);
    }

    #[test]
    fn empty_input_is_noop() {
        let mut w = MockWriter::new(vec![]);
        write_all_retrying(&mut w, b"", Duration::from_secs(5)).unwrap();
    }

    // End-to-end real-OS test against an actual non-blocking pipe — the
    // exact condition GHA inflicts on moon's stdout. Without the fix, the
    // first write of >64 KiB into a non-drained non-blocking pipe returns
    // EAGAIN and `write_all` propagates it.
    #[cfg(unix)]
    #[test]
    fn real_nonblocking_pipe_drained_slowly_succeeds() {
        use std::os::fd::FromRawFd;
        use std::thread;

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

        // Reader thread drains, but with a deliberate stall up front so
        // the pipe buffer fills and the writer hits EAGAIN.
        let reader = thread::spawn(move || {
            thread::sleep(Duration::from_millis(150));
            let mut total = 0_usize;
            let mut buf = [0_u8; 8192];
            loop {
                let n = unsafe {
                    libc::read(read_fd, buf.as_mut_ptr() as *mut _, buf.len())
                };
                if n <= 0 {
                    break;
                }
                total += n as usize;
            }
            unsafe { libc::close(read_fd) };
            total
        });

        // 1 MiB — comfortably exceeds the default 64 KiB Linux pipe
        // buffer, guaranteeing at least one EAGAIN cycle.
        let payload = vec![b'x'; 1024 * 1024];

        let mut writer = unsafe { std::fs::File::from_raw_fd(write_fd) };
        let result = write_all_retrying(&mut writer, &payload, Duration::from_secs(10));
        // Drop closes the write end so the reader's read() returns 0.
        drop(writer);

        let total_read = reader.join().expect("reader panicked");
        assert!(
            result.is_ok(),
            "write_all_retrying should ride out EAGAIN: {:?}",
            result
        );
        assert_eq!(total_read, payload.len());
    }

    // Same setup, but the reader never reads. The pipe stays full and
    // EAGAIN persists. After the (short) deadline we must surface
    // WouldBlock — not hang and not silently truncate.
    #[cfg(unix)]
    #[test]
    fn real_nonblocking_pipe_with_stuck_reader_hits_deadline() {
        use std::os::fd::FromRawFd;

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
        let mut writer = unsafe { std::fs::File::from_raw_fd(write_fd) };
        let err = write_all_retrying(&mut writer, &payload, Duration::from_millis(200))
            .expect_err("stuck reader should yield WouldBlock past deadline");
        assert_eq!(err.kind(), io::ErrorKind::WouldBlock);

        drop(writer);
        unsafe { libc::close(read_fd) };
    }
}
