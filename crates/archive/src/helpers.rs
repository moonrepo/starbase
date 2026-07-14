use crate::archive_error::ArchiveError;
use starbase_utils::fs;
use std::env;
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicUsize, Ordering};
use tracing::trace;

/// Return a unique temporary directory to mount or expand an archive
/// into. Each unpack requires its own directory, otherwise unpacks
/// running in parallel would collide with each other (mounts would
/// detach each other, and `pkgutil` refuses to expand into an existing
/// directory), so namespace by process and an incrementing counter.
pub fn next_temp_dir() -> PathBuf {
    static TEMP_ID: AtomicUsize = AtomicUsize::new(0);

    env::temp_dir().join(format!(
        "starbase-archive-{}-{}",
        process::id(),
        TEMP_ID.fetch_add(1, Ordering::Relaxed)
    ))
}

// Convert a failed command execution into an error, preferring the
// stderr output, which contains messages like
// "hdiutil: attach failed - image not recognized".
pub fn convert_command_error(command: &str, output: &process::Output) -> io::Error {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    io::Error::other(if stderr.is_empty() {
        format!("`{command}` failed ({})", output.status)
    } else {
        stderr
    })
}

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

/// Return a list of all supported compression/codec file extensions,
/// without the format-specific extensions like `tar` or `zip`,
/// regardless of which Cargo features are enabled.
pub fn get_compression_extensions() -> Vec<String> {
    vec![
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
        // compress (LZW)
        "Z".into(),
        "z".into(),
    ]
}

/// Return a list of all supported archive file extensions,
/// regardless of which Cargo features are enabled.
/// Extensions are returned *without* a leading dot.
pub fn get_supported_archive_extensions() -> Vec<String> {
    // Order is important here! Must be from most
    // specific to least specific (any entry whose suffix is
    // another entry in this list MUST come before that entry).
    let mut list = vec![
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
        // tar + compress (must precede `Z`/`z`)
        // `taz` is intentionally absent, since GNU tar maps it to gzip instead
        "tar.Z".into(),
        "tar.z".into(),
        "taZ".into(),
        "tZ".into(),
        // tar
        "tar".into(),
        // dmg
        "dmg".into(),
        // pkg
        "pkg".into(),
        // zip
        "zip".into(),
    ];
    list.extend(get_compression_extensions());
    list
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
    for ext in get_compression_extensions() {
        if let Some(stripped) = name.strip_suffix(&format!(".{ext}")) {
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

/// Strip the prefix from a path, supporting `*` wildcards in the prefix.
/// A `*` matches any number of characters within a single component, so
/// `*` matches a whole component, while partial patterns like `name-*`
/// match components starting with `name-`. Wildcards never match across
/// component separators.
pub fn strip_path_prefix<'a>(path: &'a Path, prefix: &str) -> Option<&'a Path> {
    if !prefix.contains('*') {
        return path.strip_prefix(prefix).ok();
    }

    let mut p1 = path.components();
    let mut p2 = Path::new(prefix).components();

    loop {
        // Exhaust the prefix before pulling from the path, otherwise the
        // first component of the remainder would be consumed and lost.
        let Some(expected) = p2.next() else {
            return Some(p1.as_path());
        };

        match p1.next() {
            Some(part) if matches_component(part.as_os_str(), expected.as_os_str()) => (),
            _ => return None,
        }
    }
}

// Return true if a single path component matches a single prefix
// component, where `*` matches any number of characters.
fn matches_component(part: &OsStr, pattern: &OsStr) -> bool {
    if pattern == "*" {
        return true;
    }

    // Prefixes are `&str`, so the pattern is always valid UTF-8
    let Some(pattern) = pattern.to_str() else {
        return false;
    };

    if !pattern.contains('*') {
        return part == pattern;
    }

    // Wildcards only match within valid UTF-8 components
    let Some(part) = part.to_str() else {
        return false;
    };

    // The first segment anchors to the start and the last segment to
    // the end, while middle segments match anywhere in order.
    let segments = pattern.split('*').collect::<Vec<_>>();
    let last_index = segments.len() - 1;

    let Some(mut rest) = part.strip_prefix(segments[0]) else {
        return false;
    };

    for segment in &segments[1..last_index] {
        match rest.find(segment) {
            Some(index) => rest = &rest[index + segment.len()..],
            None => return false,
        }
    }

    segments[last_index].is_empty() || rest.ends_with(segments[last_index])
}
