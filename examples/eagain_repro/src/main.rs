// Reproducer for moon issue #2465 (EAGAIN variant).
//
// Sets stdout to O_NONBLOCK (mimicking the GHA log-forwarder pipe state),
// then writes a large volume through starbase's ConsoleStream. Pipe the
// program into a slow reader so the kernel pipe buffer fills:
//
//     cargo run -p example_eagain_repro 2>err.log | (sleep 3; cat > /dev/null)
//
// Expected (buggy) output on stderr:
//     FAIL: ConsoleError::WriteFailed { ... os error 11 (EAGAIN) }
// Or on close():
//     FAIL: ConsoleError::FlushFailed { ... os error 11 }
//
// On success exits 0 silently.

use starbase_console::{ConsoleStream, ConsoleStreamType};
use std::io::Write;
use std::process::ExitCode;

fn set_stdout_nonblocking() {
    unsafe {
        let fd = libc::STDOUT_FILENO;
        let flags = libc::fcntl(fd, libc::F_GETFL, 0);
        if flags < 0 {
            panic!("fcntl F_GETFL failed");
        }
        if libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
            panic!("fcntl F_SETFL O_NONBLOCK failed");
        }
    }
}

fn main() -> ExitCode {
    if std::env::var("EAGAIN_REPRO_NONBLOCK").as_deref() != Ok("0") {
        set_stdout_nonblocking();
    }

    let stream = ConsoleStream::new(ConsoleStreamType::Stdout);

    // 16 KiB line, written ~4000 times = ~64 MiB. Linux default pipe
    // buffer is 64 KiB, so even a 1-second-late reader will EAGAIN
    // long before this finishes.
    let line: String = "x".repeat(16 * 1024);

    for i in 0..4000 {
        if let Err(e) = stream.write_line(&line) {
            eprintln!("FAIL on write_line iter {i}: {e:?}");
            // Show the inner io::Error kind / raw_os_error:
            if let starbase_console::ConsoleError::WriteFailed { error }
            | starbase_console::ConsoleError::FlushFailed { error } = &e
            {
                eprintln!("  io::ErrorKind = {:?}", error.kind());
                eprintln!("  raw_os_error = {:?}", error.raw_os_error());
            }
            return ExitCode::from(1);
        }
    }

    if let Err(e) = stream.close() {
        eprintln!("FAIL on close: {e:?}");
        if let starbase_console::ConsoleError::WriteFailed { error }
        | starbase_console::ConsoleError::FlushFailed { error } = &e
        {
            eprintln!("  io::ErrorKind = {:?}", error.kind());
            eprintln!("  raw_os_error = {:?}", error.raw_os_error());
        }
        return ExitCode::from(1);
    }

    // Force a stderr flush so the test driver sees output ordering correctly.
    let _ = std::io::stderr().flush();
    ExitCode::SUCCESS
}
