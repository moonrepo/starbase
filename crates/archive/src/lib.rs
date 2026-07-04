/// Handles standard `.gz` files.
#[cfg(feature = "gz")]
pub mod gz;
#[cfg(feature = "gz")]
mod gz_error;

/// Handles `.tar`, `.tar.bz2`, `.tar.gz`, `.tar.xz`, and `.tar.zst` files.
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

/// Returns `true` if writing to `target` would escape `root` by traversing a
/// symlink -- for example a symlink entry planted earlier in the same archive
/// that points outside the output directory (CWE-22 / CWE-59). Every already
/// existing ancestor of `target` beneath `root` is checked, since
/// `create_dir_all` and file writes would otherwise follow such a link.
#[cfg(any(feature = "tar", feature = "zip"))]
pub(crate) fn escapes_via_symlink(root: &Path, target: &Path) -> bool {
    let Ok(rel) = target.strip_prefix(root) else {
        // Not under the root at all, so treat it as unsafe.
        return true;
    };

    let mut current = root.to_path_buf();

    for component in rel.components() {
        current.push(component);

        if current
            .symlink_metadata()
            .is_ok_and(|meta| meta.file_type().is_symlink())
        {
            return true;
        }
    }

    false
}

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
        .find(|ext| file_name.ends_with(&format!(".{ext}")))
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
    // specific to least specific (any entry whose suffix is
    // another entry in this list MUST come before that entry).
    vec![
        // tar + bzip2
        "tar.bz2".into(),
        "tbz2".into(),
        "tbz".into(),
        "tz2".into(),
        // tar + gzip (must precede `gz`/`gzip`)
        "tar.gz".into(),
        "tgz".into(),
        // tar + xz
        "tar.xz".into(),
        "txz".into(),
        // tar + zstd (must precede `zstd`/`zst`)
        "tar.zstd".into(),
        "tar.zst".into(),
        "tzst".into(),
        "tzs".into(),
        // tar
        "tar".into(),
        // zip
        "zip".into(),
        // zstd
        "zstd".into(),
        "zst".into(),
        // gzip
        "gzip".into(),
        "gz".into(),
    ]
}

/// Return true if the file path has a supported archive extension.
/// This does not check against feature flags!
pub fn is_supported_archive_extension(path: &Path) -> bool {
    path.file_name()
        .and_then(|file| file.to_str())
        .is_some_and(|name| {
            get_supported_archive_extensions()
                .into_iter()
                .any(|ext| name.ends_with(&format!(".{ext}")))
        })
}

#[cfg(all(test, unix, any(feature = "tar", feature = "zip")))]
mod symlink_guard_tests {
    use super::escapes_via_symlink;
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn flags_writes_through_a_symlinked_parent() {
        let root = std::env::temp_dir().join(format!(
            "starbase-symlink-guard-{}-{}",
            std::process::id(),
            UNIX_EPOCH.elapsed().unwrap_or_default().as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();

        // A plain nested path inside the root is safe.
        assert!(!escapes_via_symlink(&root, &root.join("safe/file.txt")));

        // Plant `evil -> <outside>`, mimicking an earlier archive entry; a write
        // through it must be rejected.
        let outside = std::env::temp_dir();
        symlink(&outside, root.join("evil")).unwrap();
        assert!(escapes_via_symlink(&root, &root.join("evil/passwd")));

        // A path that isn't under the root at all is also rejected.
        assert!(escapes_via_symlink(&root, &outside.join("elsewhere")));

        fs::remove_dir_all(&root).unwrap();
    }
}
