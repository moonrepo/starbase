use std::char::REPLACEMENT_CHARACTER;
use std::ffi::OsStr;
use std::hash::{DefaultHasher, Hasher};
use std::path::{Component, Path, PathBuf};

/// Normalize separators in a path string to their OS specific separators.
/// On Unix this will be `/`, and on Windows `\`.
#[inline]
pub fn normalize_separators<T: AsRef<str>>(path: T) -> String {
    #[cfg(unix)]
    {
        path.as_ref().replace('\\', "/")
    }

    #[cfg(windows)]
    {
        path.as_ref().replace('/', "\\")
    }
}

/// Standardize separators in a path string to `/` for portability,
#[inline]
pub fn standardize_separators<T: AsRef<str>>(path: T) -> String {
    path.as_ref().replace('\\', "/")
}

/// Format the provided name for use as an executable file.
/// On Windows this will append `.exe`, on Unix used as-is.
#[inline]
pub fn exe_name<T: AsRef<str>>(name: T) -> String {
    let name = name.as_ref();

    #[cfg(unix)]
    {
        name.into()
    }

    #[cfg(windows)]
    {
        if name.ends_with(".exe") {
            name.into()
        } else {
            format!("{name}.exe")
        }
    }
}

/// Encode a value by removing invalid characters for use within a path component.
pub fn encode_component<T: AsRef<OsStr>>(value: T) -> String {
    let mut output = String::new();

    for ch in value.as_ref().to_string_lossy().chars() {
        match ch {
            '@' | '*' | REPLACEMENT_CHARACTER => {
                // Skip these
            }
            '/' | ':' => {
                output.push('-');
            }
            _ => {
                output.push(ch);
            }
        }
    }

    output.trim_matches(['-', '.']).to_owned()
}

/// Hash a value that may contain special characters into a valid path component.
pub fn hash_component<T: AsRef<OsStr>>(value: T) -> String {
    let mut hasher = DefaultHasher::default();
    hasher.write(value.as_ref().as_encoded_bytes());

    format!("{}", hasher.finish())
}

/// Clean a path by removing and flattening unnecessary path components.
pub fn clean<T: AsRef<Path>>(path: T) -> PathBuf {
    // Based on https://gitlab.com/foo-jin/clean-path
    let mut components = path.as_ref().components().peekable();

    let mut cleaned = if let Some(c @ Component::Prefix(..)) = components.peek().cloned() {
        components.next();

        PathBuf::from(c.as_os_str())
    } else {
        PathBuf::new()
    };

    let mut leading_parent_dots = 0;
    let mut component_count = 0;

    for component in components {
        match component {
            Component::Prefix(_) | Component::CurDir => {}
            Component::RootDir => {
                cleaned.push(component.as_os_str());
                component_count += 1;
            }
            Component::ParentDir => {
                if component_count == 1 && cleaned.is_absolute() {
                    // Nothing
                } else if component_count == leading_parent_dots {
                    cleaned.push("..");
                    leading_parent_dots += 1;
                    component_count += 1;
                } else {
                    cleaned.pop();
                    component_count -= 1;
                }
            }
            Component::Normal(c) => {
                cleaned.push(c);
                component_count += 1;
            }
        }
    }

    if component_count == 0 {
        cleaned.push(".");
    }

    cleaned
}

/// Return true if both provided paths are equal. Both paths will be cleaned
/// before comparison for accurate matching.
pub fn are_equal<L: AsRef<Path>, R: AsRef<Path>>(left: L, right: R) -> bool {
    clean(left) == clean(right)
}

/// Extend the native [`Path`] and [`PathBuf`] with additional functionality.
pub trait PathExt {
    /// Clean a path by removing and flattening unnecessary path components.
    fn clean(&self) -> PathBuf;

    /// Return true if the current path matches the provided path.
    /// Both paths will be cleaned before comparison for accurate matching.
    fn matches(&self, other: &Path) -> bool;
}

impl PathExt for Path {
    fn clean(&self) -> PathBuf {
        clean(self)
    }

    fn matches(&self, other: &Path) -> bool {
        are_equal(self, other)
    }
}

impl PathExt for PathBuf {
    fn clean(&self) -> PathBuf {
        clean(self)
    }

    fn matches(&self, other: &Path) -> bool {
        are_equal(self, other)
    }
}
