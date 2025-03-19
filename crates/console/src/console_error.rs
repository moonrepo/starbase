use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
#[cfg_attr(feature = "miette", derive(miette::Diagnostic))]
pub enum ConsoleError {
    #[cfg_attr(feature = "miette", diagnostic(code(console::flush_failed)))]
    #[error("Failed to flush buffered output to console.")]
    FlushFailed {
        #[source]
        error: Box<io::Error>,
    },

    #[cfg_attr(feature = "miette", diagnostic(code(console::render_failed)))]
    #[error("Failed to render user interface to console.")]
    RenderFailed {
        #[source]
        error: Box<io::Error>,
    },

    #[cfg_attr(feature = "miette", diagnostic(code(console::write_failed)))]
    #[error("Failed to write output to console.")]
    WriteFailed {
        #[source]
        error: Box<io::Error>,
    },
}
