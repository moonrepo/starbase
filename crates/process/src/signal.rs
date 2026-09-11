// https://www.math.stonybrook.edu/~ccc/dfc/dfc/signals.html
// https://sunshowers.io/posts/beyond-ctrl-c-signals/

use std::io;
use tokio::sync::broadcast::Sender;
use tracing::debug;

/// A signal that can be sent to a running child process.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SignalType {
    /// Request the process interrupt itself (`SIGINT`, `CTRL-C`).
    Interrupt,

    /// Force the process to stop immediately (`SIGKILL`, force terminate).
    Kill,

    /// Request the process quit and dump core (`SIGQUIT`, `CTRL-C`).
    Quit,

    /// Request the process terminate gracefully (`SIGTERM`, `CTRL-SHUTDOWN`).
    Terminate,
}

impl SignalType {
    /// Return the Unix signal number for this signal.
    pub fn get_code(&self) -> i32 {
        match self {
            SignalType::Interrupt => 2,  // SIGINT
            SignalType::Quit => 3,       // SIGQUIT
            SignalType::Kill => 9,       // SIGKILL
            SignalType::Terminate => 15, // SIGTERM
        }
    }
}

#[cfg(unix)]
mod unix {
    use super::*;

    /// Listen for `SIGINT`, `SIGQUIT`, and `SIGTERM`, and broadcast the
    /// first one received to `sender`. Runs until a signal arrives.
    pub async fn wait_for_signal(sender: Sender<SignalType>) {
        use tokio::signal::unix::{SignalKind, signal};

        debug!("Listening for SIGINT, SIGQUIT, and SIGTERM signals");

        let mut signal_terminate = signal(SignalKind::terminate()).unwrap();
        let mut signal_interrupt = signal(SignalKind::interrupt()).unwrap();
        let mut signal_quit = signal(SignalKind::quit()).unwrap();

        let _ = tokio::select! {
            _ = signal_terminate.recv() => {
                debug!("Received SIGTERM signal");
                sender.send(SignalType::Terminate)
            },
            _ = signal_interrupt.recv() => {
                debug!("Received SIGINT signal");
                sender.send(SignalType::Interrupt)
            },
            _ = signal_quit.recv() => {
                debug!("Received SIGQUIT signal");
                sender.send(SignalType::Quit)
            },
        };
    }

    /// Send a signal to a process by pid. A process that no longer exists
    /// is treated as already dead, not as an error.
    pub fn kill(pid: u32, signal: SignalType) -> io::Result<()> {
        let result = unsafe { libc::kill(pid as i32, signal.get_code()) };

        if result != 0 {
            let error = io::Error::last_os_error();

            // "No such process" error, so it may have been killed already
            // https://man7.org/linux/man-pages/man3/errno.3.html
            if error.raw_os_error().is_some_and(|code| code == libc::ESRCH) {
                return Ok(());
            }

            return Err(error);
        }

        Ok(())
    }

    /// Send a signal to a process group. A group that no longer exists is
    /// treated as already dead. macOS may report this state as `EPERM` when
    /// only the zombie group leader remains.
    pub(crate) fn kill_process_group(pgid: u32, signal: SignalType) -> io::Result<()> {
        let pgid = i32::try_from(pgid).map_err(io::Error::other)?;
        let result = unsafe { libc::kill(-pgid, signal.get_code()) };

        if result == 0 {
            return Ok(());
        }

        let error = io::Error::last_os_error();

        match error.raw_os_error() {
            Some(libc::ESRCH) => Ok(()),
            #[cfg(target_vendor = "apple")]
            Some(libc::EPERM) => Ok(()),
            _ => Err(error),
        }
    }
}

#[cfg(unix)]
pub use unix::*;

#[cfg(windows)]
mod windows {
    use super::*;
    use std::os::raw::c_void;
    // use windows_sys::Win32::System::Console::{CTRL_BREAK_EVENT, GenerateConsoleCtrlEvent};
    use windows_sys::Win32::System::Threading::TerminateProcess;

    /// Listen for `CTRL-C`, `CTRL-BREAK`, `CTRL-CLOSE`, and
    /// `CTRL-SHUTDOWN`, and broadcast the first one received to `sender`.
    /// Runs until a signal arrives.
    pub async fn wait_for_signal(sender: Sender<SignalType>) {
        use tokio::signal::windows;

        debug!("Listening for CTRL-C, BREAK, CLOSE, and SHUTDOWN signals");

        let mut signal_c = windows::ctrl_c().unwrap();
        let mut signal_break = windows::ctrl_break().unwrap();
        let mut signal_close = windows::ctrl_close().unwrap();
        let mut signal_shutdown = windows::ctrl_shutdown().unwrap();

        let _ = tokio::select! {
            _ = signal_c.recv() => {
                debug!("Received CTRL-C signal");
                sender.send(SignalType::Interrupt)
            },
            _ = signal_break.recv() => {
                debug!("Received CTRL-BREAK signal");
                sender.send(SignalType::Interrupt)
            },
            _ = signal_close.recv() => {
                debug!("Received CTRL-CLOSE signal");
                sender.send(SignalType::Quit)
            },
            _ = signal_shutdown.recv() => {
                debug!("Received CTRL-SHUTDOWN signal");
                sender.send(SignalType::Terminate)
            },
        };
    }

    /// A `Send` + `Sync` wrapper around a raw Windows process handle.
    #[derive(Clone)]
    pub struct RawHandle(pub *mut c_void);

    unsafe impl Send for RawHandle {}
    unsafe impl Sync for RawHandle {}

    /// Send a signal to a process by its handle. `Interrupt` is a no-op,
    /// as `CTRL-C`/`CTRL-BREAK` are delivered by the OS and can't be
    /// targeted at a specific process; every other signal terminates it.
    pub fn kill(_pid: u32, handle: RawHandle, signal: SignalType) -> io::Result<()> {
        let result = match signal {
            // https://learn.microsoft.com/en-us/windows/console/generateconsolectrlevent
            SignalType::Interrupt => {
                // Do nothing and let signals pass through natively!
                // unsafe {
                //     GenerateConsoleCtrlEvent(
                //         // We can't use CTRL_C_EVENT here, as it doesn't propagate
                //         CTRL_BREAK_EVENT,
                //         pid,
                //     )
                // }
                1
            }
            // https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-terminateprocess
            _ => unsafe { TerminateProcess(handle.0, 1) },
        };

        if result == 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(())
    }
}

#[cfg(windows)]
pub use windows::*;
