use miette::Diagnostic;
use starbase_utils::fs::FsError;
use starbase_utils::glob::GlobError;
use thiserror::Error;

#[derive(Error, Diagnostic, Debug)]
pub enum ArchiveError {
    #[error(transparent)]
    Fs(#[from] FsError),

    #[error(transparent)]
    Glob(#[from] GlobError),
}
