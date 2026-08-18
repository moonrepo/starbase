use crate::process_error::ProcessError;
use crate::shared_child::ChildExit;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::process::ExitStatus;

/// The standard library's [`Output`](std::process::Output), re-exported
/// for convenience.
pub use std::process::Output as NativeOutput;

/// A serializable summary of a process's [`Output`], suitable for caching
/// or logging. Fields that don't apply to the exit are omitted.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OutputInfo {
    /// The process exit code, if it exited normally.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,

    /// The signal that terminated the process, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal: Option<u8>,

    /// The trimmed stderr, if any was captured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,

    /// The trimmed stdout, if any was captured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
}

/// The result of running a [`Command`](crate::Command): how it exited, and
/// whatever output was captured.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Output {
    /// How the process exited.
    pub exit: ChildExit,

    /// The captured stderr, empty if not captured.
    pub stderr: Bytes,

    /// The captured stdout, empty if not captured.
    pub stdout: Bytes,
}

impl Output {
    /// Return the process exit code, if it exited normally.
    pub fn code(&self) -> Option<i32> {
        self.status().and_then(|status| status.code())
    }

    /// Return the native exit status, if the process ran to completion
    /// rather than being interrupted, killed, or terminated by a signal.
    pub fn status(&self) -> Option<ExitStatus> {
        match self.exit {
            ChildExit::Completed(status) => Some(status),
            _ => None,
        }
    }

    /// Return true if the process exited with a zero status.
    pub fn success(&self) -> bool {
        self.status().is_some_and(|status| status.success())
    }

    /// Convert to a serializable, trimmed summary suitable for caching.
    pub fn to_info(&self) -> OutputInfo {
        OutputInfo {
            exit_code: self.code(),
            signal: self
                .exit
                .signal()
                .and_then(|signal| u8::try_from(signal).ok()),
            stderr: if self.stderr.is_empty() {
                None
            } else {
                Some(output_to_trimmed_string(&self.stderr))
            },
            stdout: if self.stdout.is_empty() {
                None
            } else {
                Some(output_to_trimmed_string(&self.stdout))
            },
        }
    }

    /// Convert a failed exit into a [`ProcessError`]. When `with_message`
    /// is true, the captured stderr (or stdout if stderr is empty) is
    /// included in the error.
    pub fn to_error(&self, bin: impl AsRef<str>, with_message: bool) -> ProcessError {
        let bin = bin.as_ref().to_owned();
        let code = self.code();

        let status = match &self.exit {
            ChildExit::Completed(status) => match status.code() {
                Some(code) => format!("exit code {code}"),
                None => status.to_string(),
            },
            ChildExit::Interrupted => "interrupted".into(),
            ChildExit::Killed => "killed".into(),
            ChildExit::Terminated(signal) => format!("terminated by signal {signal}"),
        };

        if !with_message {
            return ProcessError::ExitNonZero { bin, status, code };
        }

        let mut message = output_to_trimmed_string(&self.stderr);

        if message.is_empty() {
            message = output_to_trimmed_string(&self.stdout);
        }

        // Make error message nicer to look at
        if !message.is_empty() {
            message = format!("\n\n{message}");
        }

        ProcessError::ExitNonZeroWithOutput {
            bin,
            status,
            code,
            output: message,
        }
    }
}

/// Convert raw bytes to a string, replacing invalid UTF-8 sequences.
#[inline]
pub fn output_to_string(data: &[u8]) -> String {
    String::from_utf8_lossy(data).into_owned()
}

/// Convert raw bytes to a string, replacing invalid UTF-8 sequences and
/// trimming leading and trailing whitespace.
#[inline]
pub fn output_to_trimmed_string(data: &[u8]) -> String {
    output_to_string(data).trim().to_owned()
}
