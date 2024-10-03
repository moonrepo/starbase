/// Handles standard `.gz` files.
#[cfg(feature = "gz")]
pub mod gz;
#[cfg(feature = "gz")]
mod gz_error;

/// Handles `.tar`, `.tar.bz2`, `.tar.gz`, and `.tar.xz` files.
#[cfg(feature = "tar")]
pub mod tar;
#[cfg(feature = "tar")]
mod tar_error;

/// Handles `.zip` files.
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

use starbase_utils::fs;
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

/// Extract the full extension from a file path without leading dot,
/// like `tar.gz`, instead of just `gz`.  If no file extension
/// is found, returns `None`.`
pub fn get_full_file_extension(path: &Path) -> Option<String> {
    let file_name = fs::file_name(path);

    if let Some(found) = get_supported_archive_extensions()
        .into_iter()
        .find(|ext| file_name.ends_with(ext))
    {
        return Some(found);
    }

    // This is to handle "unsupported format" scenarios
    if let Some(ext) = path.extension().and_then(|ext| ext.to_str()) {
        return Some(ext.to_owned());
    }

    None
}

/// Return a list of all supported archive file extensions,
/// regardless of which Cargo features are enabled.
pub fn get_supported_archive_extensions() -> Vec<String> {
    // Order is important here! Must be from most
    // specific to least specific!
    vec![
        "tar.gz".into(),
        "tar.xz".into(),
        "tar.bz2".into(),
        "tar".into(),
        "tgz".into(),
        "txz".into(),
        "tbz".into(),
        "tbz2".into(),
        "tz2".into(),
        "zstd".into(),
        "zst".into(),
        "zip".into(),
        "gz".into(),
    ]
}

/// Return true if the file path has a supported archive extension.
/// This does not check against feature flags!
pub fn is_supported_archive_extension(path: &Path) -> bool {
    path.file_name()
        .and_then(|file| file.to_str())
        .map_or(false, |name| {
            get_supported_archive_extensions()
                .into_iter()
                .any(|ext| name.ends_with(&ext))
        })
}
