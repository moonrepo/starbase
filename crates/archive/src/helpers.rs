use starbase_utils::fs;
use std::path::Path;
use tracing::trace;

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
        // dmg
        "dmg".into(),
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

/// Files that macOS creates on volumes to track volume metadata (Finder
/// info, Spotlight indexes, trash). They aren't part of the actual
/// contents, and some (like `.Trashes`) aren't readable without elevated
/// permissions, which would fail the extraction entirely.
const VOLUME_METADATA_FILES: &[&str] = &[
    ".DS_Store",
    ".DocumentRevisions-V100",
    ".Spotlight-V100",
    ".TemporaryItems",
    ".Trashes",
    ".fseventsd",
];

/// Copy contents that were extracted (or mounted) into the source directory
/// over to the target directory. If a prefix is provided, copy the contents
/// of that sub-directory instead. Used by formats (like dmg) that can't
/// unpack entries from a stream.
///
/// Symlinks are recreated verbatim, and macOS volume metadata files
/// (`.DS_Store`, `.Trashes`, etc) are skipped.
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

    let missing_contents = || ArchiveError::MissingArchiveContents {
        format: format.into(),
        path: source_dir.to_path_buf(),
        prefix: prefix.unwrap_or("N/A").into(),
    };

    if !source.exists() {
        return Err(missing_contents());
    }

    // The prefix may traverse a symlink pointing outside the source
    // directory (like an `Applications -> /Applications` shortcut in a
    // dmg), which would copy unrelated system contents (CWE-22).
    if prefix.is_some() {
        let escapes = match (source_dir.canonicalize(), source.canonicalize()) {
            (Ok(root), Ok(source)) => !source.starts_with(root),
            _ => true,
        };

        if escapes {
            return Err(missing_contents());
        }
    }

    if source.is_file() {
        fs::copy_file(&source, target_dir.join(fs::file_name(&source)))?;
    } else {
        copy_contents(&source, target_dir, target_dir)?;
    }

    Ok(())
}

fn copy_contents(
    source_dir: &Path,
    target_dir: &Path,
    target_root: &Path,
) -> Result<(), ArchiveError> {
    fs::create_dir_all(target_dir)?;

    for entry in fs::read_dir(source_dir)? {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        let name = entry.file_name();

        if name
            .to_str()
            .is_some_and(|name| VOLUME_METADATA_FILES.contains(&name))
        {
            continue;
        }

        let source_path = entry.path();
        let target_path = target_dir.join(&name);

        // Refuse to write through a symlink already existing in the target,
        // which could redirect the write outside the target directory.
        if escapes_via_symlink(target_root, &target_path) {
            trace!(source = ?source_path, "Skipping entry that would escape via a symlink");
            continue;
        }

        if file_type.is_dir() {
            copy_contents(&source_path, &target_path, target_root)?;
        } else if file_type.is_file() {
            fs::copy_file(&source_path, &target_path)?;
        } else if file_type.is_symlink() {
            // Recreate the symlink instead of following it, both to mirror
            // the source contents exactly (app bundles rely on internal
            // symlinks), and to avoid copying entire external directories
            // through shortcuts like `Applications -> /Applications`.
            copy_symlink(&source_path, &target_path)?;
        }
    }

    Ok(())
}

fn copy_symlink(source_path: &Path, target_path: &Path) -> Result<(), fs::FsError> {
    use fs::FsError;

    let map_error = |error| FsError::Create {
        path: target_path.to_path_buf(),
        error: Box::new(error),
    };

    let link_target = std::fs::read_link(source_path).map_err(|error| FsError::Read {
        path: source_path.to_path_buf(),
        error: Box::new(error),
    })?;

    // Overwrite anything already at the path, matching other unpackers
    fs::remove_link(target_path)?;
    fs::remove_file(target_path)?;

    #[cfg(unix)]
    std::os::unix::fs::symlink(&link_target, target_path).map_err(map_error)?;

    #[cfg(windows)]
    {
        if link_target.is_dir() {
            std::os::windows::fs::symlink_dir(&link_target, target_path).map_err(map_error)?;
        } else {
            std::os::windows::fs::symlink_file(&link_target, target_path).map_err(map_error)?;
        }
    }

    Ok(())
}
