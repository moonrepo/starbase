use std::io;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("Failed to capture process output.")]
    Capture(#[source] io::Error),

    #[error("Process output exceeded the {limit}-byte limit.")]
    OutputLimitExceeded { limit: usize },

    #[error("Process exceeded its {timeout:?} timeout.")]
    Timeout { timeout: Duration },

    #[error("Failed to write process input.")]
    WriteInput(#[source] io::Error),
}
