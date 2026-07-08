use starbase_utils::fs;
use std::path::Path;

use crate::ArchiveError;

/// Returns `true` if writing to `target` would escape `root` by traversing a
/// symlink -- for example a symlink entry planted earlier in the same archive
/// that points outside the output directory (CWE-22 / CWE-59). Every already
/// existing ancestor of `target` beneath `root` is checked, since
/// `create_dir_all` and file writes would otherwise follow such a link.
pub fn escapes_via_symlink(root: &Path, target: &Path) -> bool {
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
/// like `tar.gz`, instead of just `gz`. If no file extension
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
/// Extensions are returned *without* a leading dot.
pub fn get_supported_archive_extensions() -> Vec<String> {
    // Order is important here! Must be from most
    // specific to least specific (any entry whose suffix is
    // another entry in this list MUST come before that entry).
    vec![
        // tar + bzip2 (must precede `bz`/`bzip2`)
        "tar.bzip2".into(),
        "tar.bz2".into(),
        "tbz2".into(),
        "tbz".into(),
        "tz2".into(),
        // tar + gzip (must precede `gz`/`gzip`)
        "tar.gzip".into(),
        "tar.gz".into(),
        "tgz".into(),
        // tar + xz (must precede `xz`)
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
        // bzip2
        "bzip2".into(),
        "bz2".into(),
        // gzip
        "gzip".into(),
        "gz".into(),
        // xz
        "xz".into(),
        // zstd
        "zstd".into(),
        "zst".into(),
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

/// Remove a trailing compression extension (`.bz2`, `.gz`, `.xz`, `.zst`, etc.)
/// from the file name, returning the inner file name.
pub fn strip_compression_suffix(name: String) -> String {
    for ext in [".bz2", ".bzip2", ".gz", ".gzip", ".xz", ".zst", ".zstd"] {
        if let Some(stripped) = name.strip_suffix(ext) {
            return stripped.into();
        }
    }

    name
}

pub fn copy_extracted_contents(
    format: &str,
    source_dir: &Path,
    target_dir: &Path,
    prefix: Option<&str>,
) -> Result<(), ArchiveError> {
    let source = match prefix {
        Some(prefix) => source_dir.join(prefix),
        None => source_dir.to_path_buf(),
    };

    if !source.exists() {
        return Err(ArchiveError::MissingArchiveContents {
            format: format.into(),
            path: source_dir.to_path_buf(),
            prefix: prefix.unwrap_or("N/A").into(),
        });
    } else if source.is_file() {
        fs::copy_file(&source, target_dir.join(fs::file_name(&source)))?;
    } else {
        fs::copy_dir_all(&source, target_dir)?;
    }

    Ok(())
}
