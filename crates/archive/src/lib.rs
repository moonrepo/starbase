/// Handle `.tar`, `.tar.gz`, and `.tar.xz` files.
#[cfg(feature = "tar")]
pub mod tar;

/// Handle `.zip` files.
#[cfg(feature = "zip")]
pub mod zip;
#[cfg(feature = "zip")]
mod zip_error;

mod archive;
mod archive_error;
mod tree_differ;

pub use archive::*;
pub use archive_error::*;
pub use tree_differ::*;

use std::path::Path;

/// Join a file name from a list of parts, removing any empty parts.
pub fn join_file_name<I, V>(parts: I) -> String
where
    I: IntoIterator<Item = V>,
    V: AsRef<str>,
{
    // Use native path utils to join the paths, so we can ensure
    // the parts are joined correctly within the archive!
    parts
        .into_iter()
        .filter_map(|p| {
            let p = p.as_ref();

            if p.is_empty() {
                None
            } else {
                Some(p.to_owned())
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Return true if the file path has a supported archive extension.
/// This does not check against feature flags!
pub fn is_supported_archive_extension(path: &Path) -> bool {
    path.extension()
        .map(|ext| {
            ext == "tar"
                || ext == "tgz"
                || ext == "gz"
                || ext == "txz"
                || ext == "xz"
                || ext == "zstd"
                || ext == "zst"
                || ext == "zip"
        })
        .unwrap_or(false)
}
