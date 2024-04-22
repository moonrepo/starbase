use starbase_styles::{Style, Stylize};
use std::path::PathBuf;
use thiserror::Error;

#[cfg(not(feature = "miette"))]
#[derive(Error, Debug)]
pub enum FsError {
    #[error("Failed to copy {} to {}.\n{error}", .from.style(Style::Path), .to.style(Style::Path))]
    Copy {
        from: PathBuf,
        to: PathBuf,
        #[source]
        error: Box<std::io::Error>,
    },

    #[error("Failed to create {}.\n{error}", .path.style(Style::Path))]
    Create {
        path: PathBuf,
        #[source]
        error: Box<std::io::Error>,
    },

    #[error("Failed to lock {}.\n{error}", .path.style(Style::Path))]
    Lock {
        path: PathBuf,
        #[source]
        error: Box<std::io::Error>,
    },

    #[error("Failed to update permissions for {}.\n{error}", .path.style(Style::Path))]
    Perms {
        path: PathBuf,
        #[source]
        error: Box<std::io::Error>,
    },

    #[error("Failed to read path {}.\n{error}", .path.style(Style::Path))]
    Read {
        path: PathBuf,
        #[source]
        error: Box<std::io::Error>,
    },

    #[error("Failed to remove path {}.\n{error}", .path.style(Style::Path))]
    Remove {
        path: PathBuf,
        #[source]
        error: Box<std::io::Error>,
    },

    #[error("A directory is required for path {}.", .path.style(Style::Path))]
    RequireDir { path: PathBuf },

    #[error("Failed to rename {} to {}.\n{error}", .from.style(Style::Path), .to.style(Style::Path))]
    Rename {
        from: PathBuf,
        to: PathBuf,
        #[source]
        error: Box<std::io::Error>,
    },

    #[error("Failed to unlock {}.\n{error}", .path.style(Style::Path))]
    Unlock {
        path: PathBuf,
        #[source]
        error: Box<std::io::Error>,
    },

    #[error("Failed to write {}.\n{error}", .path.style(Style::Path))]
    Write {
        path: PathBuf,
        #[source]
        error: Box<std::io::Error>,
    },
}

#[cfg(feature = "miette")]
#[derive(Error, Debug, miette::Diagnostic)]
pub enum FsError {
    #[diagnostic(code(fs::copy), help("Does the source file exist?"))]
    #[error("Failed to copy {} to {}.", .from.style(Style::Path), .to.style(Style::Path))]
    Copy {
        from: PathBuf,
        to: PathBuf,
        #[source]
        error: Box<std::io::Error>,
    },

    #[diagnostic(code(fs::create))]
    #[error("Failed to create {}.", .path.style(Style::Path))]
    Create {
        path: PathBuf,
        #[source]
        error: Box<std::io::Error>,
    },

    #[diagnostic(code(fs::lock))]
    #[error("Failed to lock {}.", .path.style(Style::Path))]
    Lock {
        path: PathBuf,
        #[source]
        error: Box<std::io::Error>,
    },

    #[diagnostic(code(fs::perms))]
    #[error("Failed to update permissions for {}.", .path.style(Style::Path))]
    Perms {
        path: PathBuf,
        #[source]
        error: Box<std::io::Error>,
    },

    #[diagnostic(code(fs::read))]
    #[error("Failed to read path {}.", .path.style(Style::Path))]
    Read {
        path: PathBuf,
        #[source]
        error: Box<std::io::Error>,
    },

    #[diagnostic(code(fs::remove))]
    #[error("Failed to remove path {}.", .path.style(Style::Path))]
    Remove {
        path: PathBuf,
        #[source]
        error: Box<std::io::Error>,
    },

    #[diagnostic(code(fs::require_dir))]
    #[error("A directory is required for path {}.", .path.style(Style::Path))]
    RequireDir { path: PathBuf },

    #[diagnostic(code(fs::rename), help("Does the source file exist?"))]
    #[error("Failed to rename {} to {}.", .from.style(Style::Path), .to.style(Style::Path))]
    Rename {
        from: PathBuf,
        to: PathBuf,
        #[source]
        error: Box<std::io::Error>,
    },

    #[diagnostic(code(fs::unlock))]
    #[error("Failed to unlock {}.", .path.style(Style::Path))]
    Unlock {
        path: PathBuf,
        #[source]
        error: Box<std::io::Error>,
    },

    #[diagnostic(code(fs::write), help("Does the parent directory exist?"))]
    #[error("Failed to write {}.", .path.style(Style::Path))]
    Write {
        path: PathBuf,
        #[source]
        error: Box<std::io::Error>,
    },
}
