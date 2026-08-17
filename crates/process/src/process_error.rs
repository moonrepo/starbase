use crate::Output;
use miette::Diagnostic;
use starbase_styles::{Style, Stylize};
use std::time::Duration;
use thiserror::Error;

/// Errors that can occur while spawning, running, or communicating with a
/// child process.
#[derive(Error, Debug, Diagnostic)]
pub enum ProcessError {
    /// Failed to spawn the process, or to read its captured output.
    #[diagnostic(code(process::capture::failed))]
    #[error(
        "Failed to execute {} and capture output.",
        .bin.style(Style::Shell),
        // .error.to_string().style(Style::MutedLight),
    )]
    Capture {
        /// The binary name that was executed.
        bin: String,
        #[source]
        /// The underlying I/O error.
        error: Box<std::io::Error>,
    },

    /// Process cleanup failed after another execution failure.
    #[diagnostic(code(process::cleanup::failed))]
    #[error(
        "Failed to clean up process {} after: {failure}",
        .bin.style(Style::Shell),
    )]
    Cleanup {
        /// The binary name that was executed.
        bin: String,
        #[source]
        /// The cleanup I/O error.
        error: Box<std::io::Error>,
        /// The original execution failure.
        failure: Box<ProcessError>,
        /// Output captured before cleanup failed.
        output: Option<Output>,
    },

    /// The process exceeded the combined stdout and stderr byte limit.
    #[diagnostic(code(process::output_limit))]
    #[error(
        "Process {} output exceeded the {limit}-byte limit.",
        .bin.style(Style::Shell),
    )]
    OutputLimitExceeded {
        /// The binary name that was executed.
        bin: String,
        /// The configured combined byte limit.
        limit: usize,
        /// Output captured before the limit was exceeded.
        output: Option<Output>,
    },

    /// The process exited but its output pipes did not close in time.
    #[diagnostic(code(process::output_drain_timeout))]
    #[error(
        "Process {} output did not drain within {timeout:?}.",
        .bin.style(Style::Shell),
    )]
    OutputDrainTimeout {
        /// The binary name that was executed.
        bin: String,
        /// The configured output-drain timeout.
        timeout: Duration,
        /// Output captured before the drain deadline.
        output: Option<Output>,
    },

    /// The process exceeded its execution timeout.
    #[diagnostic(code(process::timeout))]
    #[error(
        "Process {} exceeded its {timeout:?} timeout.",
        .bin.style(Style::Shell),
    )]
    Timeout {
        /// The binary name that was executed.
        bin: String,
        /// The configured execution timeout.
        timeout: Duration,
        /// Output captured before the execution deadline.
        output: Option<Output>,
    },

    /// The process exited with a non-zero status, without captured output.
    #[diagnostic(code(process::failed))]
    #[error(
        "Process {} failed: {status}",
        .bin.style(Style::Shell),
    )]
    ExitNonZero {
        /// The binary name that was executed.
        bin: String,
        /// A human-readable description of the exit status, e.g.
        /// `"exit code 1"` or `"terminated by signal 15"`.
        status: String,
        /// The process exit code, if it exited normally.
        code: Option<i32>,
    },

    /// The process exited with a non-zero status, with captured output.
    #[diagnostic(code(process::failed))]
    #[error(
        "Process {} failed: {status} {}",
        .bin.style(Style::Shell),
        .output.style(Style::MutedLight),
    )]
    ExitNonZeroWithOutput {
        /// The binary name that was executed.
        bin: String,
        /// A human-readable description of the exit status, e.g.
        /// `"exit code 1"` or `"terminated by signal 15"`.
        status: String,
        /// The process exit code, if it exited normally.
        code: Option<i32>,
        /// The captured stderr, or stdout if stderr was empty.
        output: String,
    },

    /// Failed to spawn the process, or to stream its output to the console.
    #[diagnostic(code(process::stream::failed))]
    #[error(
        "Failed to execute {} and stream output.",
        .bin.style(Style::Shell),
        // .error.to_string().style(Style::MutedLight),
    )]
    Stream {
        /// The binary name that was executed.
        bin: String,
        #[source]
        /// The underlying I/O error.
        error: Box<std::io::Error>,
    },

    /// Failed to spawn the process, or to stream and capture its output.
    #[diagnostic(code(process::capture_stream::failed))]
    #[error(
        "Failed to execute {} and stream and capture output.",
        .bin.style(Style::Shell),
        // .error.to_string().style(Style::MutedLight),
    )]
    StreamCapture {
        /// The binary name that was executed.
        bin: String,
        #[source]
        /// The underlying I/O error.
        error: Box<std::io::Error>,
    },

    /// Failed to write buffered input to the process's stdin.
    #[diagnostic(code(process::stdin::failed))]
    #[error(
        "Failed to write stdin to {}.",
        .bin.style(Style::Shell),
        // .error.to_string().style(Style::MutedLight),
    )]
    WriteInput {
        /// The binary name that was executed.
        bin: String,
        #[source]
        /// The underlying I/O error.
        error: Box<std::io::Error>,
    },
}

impl ProcessError {
    /// Return the underlying process exit code, if this error represents a
    /// process that exited with a non-zero status (and not a signal).
    pub fn get_exit_code(&self) -> Option<i32> {
        match self {
            Self::ExitNonZero { code, .. } | Self::ExitNonZeroWithOutput { code, .. } => *code,
            _ => None,
        }
    }
}
