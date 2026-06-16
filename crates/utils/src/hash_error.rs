use crate::fs_error::FsError;
use thiserror::Error;

/// Hash errors.
#[cfg(not(feature = "miette"))]
#[derive(Error, Debug)]
pub enum HashError {
    #[error(transparent)]
    Fs(#[from] Box<FsError>),

    #[error("Failed to hash bytes from readable stream.\n{error}")]
    ReadStream {
        #[source]
        error: Box<std::io::Error>,
    },
}

/// Hash errors.
#[cfg(feature = "miette")]
#[derive(Error, Debug, miette::Diagnostic)]
pub enum HashError {
    #[error(transparent)]
    Fs(#[from] Box<FsError>),

    #[diagnostic(code(hash::read_stream))]
    #[error("Failed to hash bytes from readable stream.")]
    ReadStream {
        #[source]
        error: Box<std::io::Error>,
    },
}

impl From<FsError> for HashError {
    fn from(e: FsError) -> HashError {
        HashError::Fs(Box::new(e))
    }
}
