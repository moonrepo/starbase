mod capture;
mod error;

pub use capture::{CaptureOptions, FileOutput, Output, capture_output, capture_output_to_files};
pub use error::CaptureError;
