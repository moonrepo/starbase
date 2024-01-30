use starbase_styles::{Style, Stylize};
use std::path::PathBuf;
use thiserror::Error;
use wax::BuildError;

#[cfg(not(feature = "miette"))]
#[derive(Error, Debug)]
pub enum GlobError {
    #[error("Failed to create glob from pattern {}.\n{error}", .glob.style(Style::File))]
    Create {
        glob: String,
        #[source]
        error: Box<BuildError>,
    },

    #[error("Failed to normalize glob path {}.", .path.style(Style::Path))]
    InvalidPath { path: PathBuf },
}

#[cfg(feature = "miette")]
#[derive(Error, Debug, miette::Diagnostic)]
pub enum GlobError {
    #[diagnostic(code(glob::create))]
    #[error("Failed to create glob from pattern {}.", .glob.style(Style::File))]
    Create {
        glob: String,
        #[source]
        error: Box<BuildError>,
    },

    #[diagnostic(code(glob::invalid_path))]
    #[error("Failed to normalize glob path {}.", .path.style(Style::Path))]
    InvalidPath { path: PathBuf },
}
