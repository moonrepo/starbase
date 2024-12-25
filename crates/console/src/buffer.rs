use crate::stream::ConsoleStreamType;
use parking_lot::Mutex;
use std::io::{self, Write};
use std::mem;
use std::sync::{mpsc, Arc};
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

    let data = mem::take(buffer);

    match stream {
        ConsoleStreamType::Stderr => io::stderr().lock().write_all(&data),
        ConsoleStreamType::Stdout => io::stdout().lock().write_all(&data),
    }
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
