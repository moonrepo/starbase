use miette::Diagnostic;
use starbase_utils::fs::FsError;
use starbase_utils::glob::GlobError;
use thiserror::Error;

#[derive(Error, Diagnostic, Debug)]
pub enum ArchiveError {
    #[diagnostic(transparent)]
    #[error(transparent)]
    Fs(#[from] FsError),

    #[diagnostic(transparent)]
    #[error(transparent)]
    Glob(#[from] GlobError),

    #[cfg(feature = "tar")]
    #[diagnostic(transparent)]
    #[error(transparent)]
    Tar(#[from] crate::tar::TarError),

    #[cfg(feature = "zip")]
    #[diagnostic(transparent)]
    #[error(transparent)]
    Zip(#[from] crate::zip::ZipError),
}
