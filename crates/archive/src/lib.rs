/// Compression codecs (`gz`, `bz2`, `xz`, `zstd`) that wrap read/write streams.
pub mod codecs;

/// Handles single files passed through a codec, like `.gz` or `.zst`.
pub mod file;
mod file_error;

/// Handles macOS dmg files.
#[cfg(feature = "dmg")]
pub mod dmg;
#[cfg(feature = "dmg")]
mod dmg_error;

/// Handles tarball files.
#[cfg(feature = "tar")]
pub mod tar;
#[cfg(feature = "tar")]
mod tar_error;

/// Handles zip files.
#[cfg(feature = "zip")]
pub mod zip;
#[cfg(feature = "zip")]
mod zip_error;

mod archive;
mod archive_error;
mod helpers;

pub use archive::*;
pub use archive_error::*;
pub use helpers::*;
