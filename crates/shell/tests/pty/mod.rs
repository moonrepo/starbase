// A minimal pty harness for driving a real interactive shell.
//
// Interactive triggers (prompt hooks, elvish's `edit:` module, nu's
// `pre_prompt`) never fire without a terminal, so the non-interactive E2E
// tests cannot reach them. This spawns the shell on the slave side of a pty
// and talks to it the way a user would.
//
// Reads are marker driven rather than timed: every step is a file that the
// shell is told to evaluate, and the harness waits for the marker that file
// prints. The typed line only ever contains the file path, so an echoed
// command can never be mistaken for the output it produces.

#![allow(dead_code)]

use std::ffi::c_int;
use std::fs::File;
use std::io::{ErrorKind, Read, Write};
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// How long to wait for a marker before giving up. Generous, since it is only
/// reached when something is wrong, and CI machines can be slow to start a
/// shell. `STARBASE_PTY_TIMEOUT` overrides it, in seconds.
fn timeout() -> Duration {
    Duration::from_secs(
        std::env::var("STARBASE_PTY_TIMEOUT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(20),
    )
}

pub struct Pty {
    child: Child,
    master: File,
    transcript: String,
}

impl Pty {
    /// Spawn the program on a pty, or return `None` when it is not installed,
    /// matching how the non-interactive tests skip a missing shell.
    pub fn spawn(program: &str, args: &[&str]) -> Option<Self> {
        Self::spawn_with_term(program, args, "dumb")
    }

    /// Some line editors need a terminal with capabilities, and others hang
    /// querying one, so the type is chosen per shell.
    pub fn spawn_with_term(program: &str, args: &[&str], term: &str) -> Option<Self> {
        if !is_installed(program) {
            let required = std::env::var("STARBASE_REQUIRED_SHELLS").unwrap_or_default();

            if required.split(',').any(|name| name.trim() == program) {
                panic!("{program} is required for E2E tests but was not found on PATH");
            }

            println!("{program} not found on PATH, skipping");

            return None;
        }

        let (master, slave) = open_pty();

        // Every prompt is redrawn as the line is edited, so a narrow terminal
        // wraps the transcript mid-marker
        set_window_size(&master, 200, 50);

        let child = unsafe {
            Command::new(program)
                .args(args)
                .env("TERM", term)
                .env("NO_COLOR", "1")
                .stdin(stdio(slave))
                .stdout(stdio(slave))
                .stderr(stdio(slave))
                // A pty needs a session of its own, otherwise the shell has no
                // controlling terminal and refuses to go interactive
                .pre_exec(move || {
                    if libc::setsid() < 0 {
                        return Err(std::io::Error::last_os_error());
                    }

                    if libc::ioctl(slave, libc::TIOCSCTTY as _, 0) < 0 {
                        return Err(std::io::Error::last_os_error());
                    }

                    Ok(())
                })
                .spawn()
                .unwrap_or_else(|error| panic!("failed to spawn {program}: {error}"))
        };

        unsafe { libc::close(slave) };

        set_non_blocking(&master);

        Some(Self {
            child,
            master,
            transcript: String::new(),
        })
    }

    /// Type a line, as a user would. Enter is a carriage return, since a line
    /// editor reads the terminal in raw mode and never sees the translation a
    /// cooked terminal would do.
    pub fn send(&mut self, line: &str) {
        write!(self.master, "{line}\r").expect("failed to write to pty");
        self.master.flush().expect("failed to flush pty");
    }

    /// Read until the marker shows up in the output, and return everything
    /// read so far with the escape sequences removed.
    pub fn wait_for(&mut self, marker: &str) -> String {
        let deadline = Instant::now() + timeout();

        loop {
            self.read();

            let output = clean(&self.transcript);

            if output.contains(marker) {
                return output;
            }

            if Instant::now() > deadline {
                panic!("timed out waiting for `{marker}`, transcript:\n{output}");
            }

            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Evaluate a file in the running session and wait for the marker it
    /// prints on its last line.
    pub fn run(&mut self, command: &str, marker: &str) -> String {
        self.send(command);
        self.wait_for(marker)
    }

    /// Send the command until its marker comes back, and return the cleaned
    /// transcript. A shell that is busy, or still starting up, drops whatever
    /// was typed instead of buffering it, so a step is only reliable once its
    /// own output has been seen.
    pub fn sync(&mut self, command: &str, marker: &str) -> String {
        let deadline = Instant::now() + timeout();

        loop {
            self.send(command);

            // Long enough that a slow shell is not typed at twice, since a
            // line editor that has not processed the first Enter appends the
            // retry to the line it is still holding
            let attempt = Instant::now() + Duration::from_secs(2);

            while Instant::now() < attempt {
                self.read();

                let output = clean(&self.transcript);

                if output.contains(marker) {
                    return output;
                }

                std::thread::sleep(Duration::from_millis(10));
            }

            assert!(
                Instant::now() < deadline,
                "timed out waiting for `{marker}`, transcript:\n{}",
                clean(&self.transcript)
            );
        }
    }

    /// Wait for the shell to answer once before anything else is typed.
    pub fn wait_until_ready(&mut self, command: &str, marker: &str) {
        self.settle();
        self.sync(command, marker);
    }

    /// Wait for the output to stop. Typing into a shell that is still loading
    /// its profile leaves the line sitting in the editor, where a retry is
    /// appended to it rather than replacing it.
    fn settle(&mut self) {
        let deadline = Instant::now() + timeout();
        let mut quiet_since = Instant::now();
        let mut length = 0;

        while Instant::now() < deadline {
            self.read();

            if self.transcript.len() != length {
                length = self.transcript.len();
                quiet_since = Instant::now();
            } else if quiet_since.elapsed() > Duration::from_millis(750) {
                return;
            }

            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Pty {
    /// Drain whatever the shell has written, answering the cursor position
    /// requests a line editor blocks on.
    fn read(&mut self) {
        let mut buffer = [0_u8; 4096];

        match self.master.read(&mut buffer) {
            Ok(0) => {}
            Ok(count) => {
                self.transcript
                    .push_str(&String::from_utf8_lossy(&buffer[..count]));

                if self.transcript.contains("\x1b[6n") {
                    self.transcript = self.transcript.replace("\x1b[6n", "");
                    self.send("\x1b[1;1R");
                }
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {}
            Err(error) => panic!("failed to read from pty: {error}"),
        }
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        let _ = self.child.kill();

        // Reaping is best effort and bounded. A shell blocked writing to a
        // master nobody reads from any more must never wedge the test run
        let deadline = Instant::now() + Duration::from_secs(5);

        loop {
            match self.child.try_wait() {
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                _ => break,
            }
        }
    }
}

fn is_installed(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

fn open_pty() -> (File, RawFd) {
    let mut master: c_int = -1;
    let mut slave: c_int = -1;

    let result = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };

    assert!(result >= 0, "failed to open a pty");

    (unsafe { File::from_raw_fd(master) }, slave)
}

fn set_window_size(master: &File, columns: u16, rows: u16) {
    let size = libc::winsize {
        ws_row: rows,
        ws_col: columns,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    unsafe { libc::ioctl(master.as_raw_fd(), libc::TIOCSWINSZ as _, &size) };
}

fn set_non_blocking(master: &File) {
    unsafe {
        let flags = libc::fcntl(master.as_raw_fd(), libc::F_GETFL);
        libc::fcntl(master.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK);
    }
}

/// Strip the escape sequences and carriage returns a line editor emits, so
/// that assertions can be written against the visible text.
fn clean(transcript: &str) -> String {
    let mut output = String::with_capacity(transcript.len());
    let mut chars = transcript.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\r' => {}
            '\x1b' => match chars.next() {
                // CSI, ends on the first byte in the @ to ~ range
                Some('[') => {
                    for ch in chars.by_ref() {
                        if ('\x40'..='\x7e').contains(&ch) {
                            break;
                        }
                    }
                }
                // OSC, ends on a bell or a string terminator
                Some(']') => {
                    while let Some(ch) = chars.next() {
                        if ch == '\x07' {
                            break;
                        }

                        if ch == '\x1b' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                }
                _ => {}
            },
            _ => output.push(ch),
        }
    }

    output
}

/// Each standard stream needs a descriptor of its own, since `Stdio` closes
/// the one it owns.
fn stdio(fd: RawFd) -> Stdio {
    let duplicate = unsafe { libc::dup(fd) };

    assert!(duplicate >= 0, "failed to duplicate the pty");

    unsafe { Stdio::from_raw_fd(duplicate) }
}
