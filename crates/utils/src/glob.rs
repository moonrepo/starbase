use std::fmt::Debug;
use std::sync::{LazyLock, RwLock};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};
use tracing::instrument;
use wax::{Any, LinkBehavior, Pattern};

pub use crate::glob_error::GlobError;
pub use wax::{self, Glob};

static GLOBAL_NEGATIONS: LazyLock<RwLock<Vec<&'static str>>> = LazyLock::new(|| {
    RwLock::new(vec![
        "**/.{git,svn}/**",
        "**/.DS_Store",
        "**/node_modules/**",
    ])
});

/// Add global negated patterns to all glob sets and walking operations.
pub fn add_global_negations<I>(patterns: I)
where
    I: IntoIterator<Item = &'static str>,
{
    let mut negations = GLOBAL_NEGATIONS.write().unwrap();
    negations.extend(patterns);
}

/// Set global negated patterns to be used by all glob sets and walking operations.
/// This will overwrite any existing global negated patterns.
pub fn set_global_negations<I>(patterns: I)
where
    I: IntoIterator<Item = &'static str>,
{
    let mut negations = GLOBAL_NEGATIONS.write().unwrap();
    negations.clear();
    negations.extend(patterns);
}

/// Match values against a set of glob patterns.
pub struct GlobSet<'glob> {
    expressions: Any<'glob>,
    negations: Any<'glob>,
    enabled: bool,
}

impl GlobSet<'_> {
    /// Create a new glob set from the list of patterns.
    /// Negated patterns must start with `!`.
    pub fn new<'new, I, V>(patterns: I) -> Result<GlobSet<'new>, GlobError>
    where
        I: IntoIterator<Item = &'new V> + Debug,
        V: AsRef<str> + 'new + ?Sized,
    {
        let (expressions, negations) = split_patterns(patterns);

        GlobSet::new_split(expressions, negations)
    }

    /// Create a new owned/static glob set from the list of patterns.
    /// Negated patterns must start with `!`.
    pub fn new_owned<'new, I, V>(patterns: I) -> Result<GlobSet<'static>, GlobError>
    where
        I: IntoIterator<Item = &'new V> + Debug,
        V: AsRef<str> + 'new + ?Sized,
    {
        let (expressions, negations) = split_patterns(patterns);

        GlobSet::new_split_owned(expressions, negations)
    }

    /// Create a new glob set with explicitly separate expressions and negations.
    /// Negated patterns must not start with `!`.
    pub fn new_split<'new, I1, V1, I2, V2>(
        expressions: I1,
        negations: I2,
    ) -> Result<GlobSet<'new>, GlobError>
    where
        I1: IntoIterator<Item = &'new V1>,
        V1: AsRef<str> + 'new + ?Sized,
        I2: IntoIterator<Item = &'new V2>,
        V2: AsRef<str> + 'new + ?Sized,
    {
        let mut ex = vec![];
        let mut ng = vec![];
        let mut count = 0;

        for pattern in expressions.into_iter() {
            ex.push(create_glob(pattern.as_ref())?);
            count += 1;
        }

        for pattern in negations.into_iter() {
            ng.push(create_glob(pattern.as_ref())?);
            count += 1;
        }

        let global_negations = GLOBAL_NEGATIONS.read().unwrap();

        for pattern in global_negations.iter() {
            ng.push(create_glob(pattern)?);
            count += 1;
        }

        Ok(GlobSet {
            expressions: wax::any(ex).unwrap(),
            negations: wax::any(ng).unwrap(),
            enabled: count > 0,
        })
    }

    /// Create a new owned/static glob set with explicitly separate expressions and negations.
    /// Negated patterns must not start with `!`.
    pub fn new_split_owned<'new, I1, V1, I2, V2>(
        expressions: I1,
        negations: I2,
    ) -> Result<GlobSet<'static>, GlobError>
    where
        I1: IntoIterator<Item = &'new V1>,
        V1: AsRef<str> + 'new + ?Sized,
        I2: IntoIterator<Item = &'new V2>,
        V2: AsRef<str> + 'new + ?Sized,
    {
        let mut ex = vec![];
        let mut ng = vec![];
        let mut count = 0;

        for pattern in expressions.into_iter() {
            ex.push(create_glob(pattern.as_ref())?.into_owned());
            count += 1;
        }

        for pattern in negations.into_iter() {
            ng.push(create_glob(pattern.as_ref())?.into_owned());
            count += 1;
        }

        let global_negations = GLOBAL_NEGATIONS.read().unwrap();

        for pattern in global_negations.iter() {
            ng.push(create_glob(pattern)?.into_owned());
            count += 1;
        }

        Ok(GlobSet {
            expressions: wax::any(ex).unwrap(),
            negations: wax::any(ng).unwrap(),
            enabled: count > 0,
        })
    }

    /// Return true if the path matches the negated patterns.
    pub fn is_negated<P: AsRef<OsStr>>(&self, path: P) -> bool {
        self.negations.is_match(path.as_ref())
    }

    /// Return true if the path matches the non-negated patterns.
    pub fn is_match<P: AsRef<OsStr>>(&self, path: P) -> bool {
        self.expressions.is_match(path.as_ref())
    }

    /// Return true if the path matches the glob patterns,
    /// while taking into account negated patterns.
    pub fn matches<P: AsRef<OsStr>>(&self, path: P) -> bool {
        if !self.enabled {
            return false;
        }

        let path = path.as_ref();

        if self.is_negated(path) {
            return false;
        }

        self.is_match(path)
    }
}

/// Parse and create a [`Glob`] instance from the borrowed string pattern.
/// If parsing fails, a [`GlobError`] is returned.
#[inline]
#[instrument]
pub fn create_glob(pattern: &str) -> Result<Glob<'_>, GlobError> {
    Glob::new(pattern).map_err(|error| GlobError::Create {
        glob: pattern.to_owned(),
        error: Box::new(error),
    })
}

/// Return true if the provided string looks like a glob pattern.
/// This is not exhaustive and may be inaccurate.
#[inline]
#[instrument]
pub fn is_glob<T: AsRef<str> + Debug>(value: T) -> bool {
    let value = value.as_ref();

    if value.contains("**") {
        return true;
    }

    let single_values = vec!['*', '?', '!'];
    let paired_values = vec![('{', '}'), ('[', ']')];
    let mut bytes = value.bytes();
    let mut is_escaped = |index: usize| {
        if index == 0 {
            return false;
        }

        bytes.nth(index - 1).unwrap_or(b' ') == b'\\'
    };

    for single in single_values {
        if !value.contains(single) {
            continue;
        }

        if let Some(index) = value.find(single) {
            if !is_escaped(index) {
                return true;
            }
        }
    }

    for (open, close) in paired_values {
        if !value.contains(open) || !value.contains(close) {
            continue;
        }

        if let Some(index) = value.find(open) {
            if !is_escaped(index) {
                return true;
            }
        }
    }

    false
}

/// Normalize a glob-based file path to use forward slashes. If the path contains
/// invalid UTF-8 characters, a [`GlobError`] is returned.
#[inline]
pub fn normalize<T: AsRef<Path>>(path: T) -> Result<String, GlobError> {
    let path = path.as_ref();

    match path.to_str() {
        Some(p) => Ok(p.replace('\\', "/")),
        None => Err(GlobError::InvalidPath {
            path: path.to_path_buf(),
        }),
    }
}

/// Split a list of glob patterns into separate non-negated and negated patterns.
/// Negated patterns must start with `!`.
#[inline]
#[instrument]
pub fn split_patterns<'glob, I, V>(patterns: I) -> (Vec<&'glob str>, Vec<&'glob str>)
where
    I: IntoIterator<Item = &'glob V> + Debug,
    V: AsRef<str> + 'glob + ?Sized,
{
    let mut expressions = vec![];
    let mut negations = vec![];

    for pattern in patterns {
        let mut negate = false;
        let mut value = pattern.as_ref();

        while value.starts_with('!') || value.starts_with('/') {
            if let Some(neg) = value.strip_prefix('!') {
                negate = true;
                value = neg;
            } else if let Some(exp) = value.strip_prefix('/') {
                value = exp;
            }
        }

        value = value.trim_start_matches("./");

        if negate {
            negations.push(value);
        } else {
            expressions.push(value);
        }
    }

    (expressions, negations)
}

/// Walk the file system starting from the provided directory, and return all files and directories
/// that match the provided glob patterns. Use [`walk_files`] if you only want to return files.
#[inline]
#[instrument]
pub fn walk<'glob, P, I, V>(base_dir: P, patterns: I) -> Result<Vec<PathBuf>, GlobError>
where
    P: AsRef<Path> + Debug,
    I: IntoIterator<Item = &'glob V> + Debug,
    V: AsRef<str> + 'glob + ?Sized,
{
    let (expressions, mut negations) = split_patterns(patterns);
    negations.extend(GLOBAL_NEGATIONS.read().unwrap().iter());

    let mut paths = vec![];

    for expression in expressions {
        for entry in create_glob(expression)?
            .walk_with_behavior(base_dir.as_ref(), LinkBehavior::ReadFile)
            .not(negations.clone())
            .unwrap()
        {
            match entry {
                Ok(e) => {
                    paths.push(e.into_path());
                }
                Err(_) => {
                    // Will crash if the file doesn't exist
                    continue;
                }
            };
        }
    }

    Ok(paths)
}

/// Walk the file system starting from the provided directory, and return all files
/// that match the provided glob patterns. Use [`walk`] if you need directories as well.
#[inline]
pub fn walk_files<'glob, P, I, V>(base_dir: P, patterns: I) -> Result<Vec<PathBuf>, GlobError>
where
    P: AsRef<Path> + Debug,
    I: IntoIterator<Item = &'glob V> + Debug,
    V: AsRef<str> + 'glob + ?Sized,
{
    let paths = walk(base_dir, patterns)?;

    Ok(paths
        .into_iter()
        .filter(|p| p.is_file())
        .collect::<Vec<_>>())
}

/// Walk the file system starting from the provided directory, and return all files and directories
/// that match the provided glob patterns. Use [`walk_files_fast`] if you only want to return files.
#[inline]
#[instrument]
pub fn walk_fast<'glob, P, I, V>(base_dir: P, patterns: I) -> Result<Vec<PathBuf>, GlobError>
where
    P: AsRef<Path> + Debug,
    I: IntoIterator<Item = &'glob V> + Debug,
    V: AsRef<str> + 'glob + ?Sized,
{
    let mut paths = vec![];
    let base_dir = base_dir.as_ref();

    // let globset = GlobSet::new_owned(patterns)?;
    // let walker = jwalk::WalkDir::new(base_dir)
    //     .follow_links(false)
    //     .skip_hidden(false)
    //     .process_read_dir(move |_depth, _dir_path, _read_dir_state, children| {
    //         children.retain(|entry_result| {
    //             entry_result
    //                 .as_ref()
    //                 .map(|entry| {
    //                     if entry.file_type().is_dir() {
    //                         true
    //                     } else {
    //                         globset.matches(entry.path())
    //                     }
    //                 })
    //                 .unwrap_or(false)
    //         });
    //     });

    let globset = GlobSet::new(patterns)?;

    for entry in jwalk::WalkDir::new(base_dir)
        .follow_links(false)
        .skip_hidden(false)
    {
        match entry {
            Ok(e) => {
                let path = e.path();

                if globset.matches(&path) {
                    paths.push(path);
                }
            }
            Err(_) => {
                // Will crash if the file doesn't exist
                continue;
            }
        };
    }

    Ok(paths)
}

/// Walk the file system starting from the provided directory, and return all files
/// that match the provided glob patterns. Use [`walk_fast`] if you need directories as well.
#[inline]
pub fn walk_files_fast<'glob, P, I, V>(base_dir: P, patterns: I) -> Result<Vec<PathBuf>, GlobError>
where
    P: AsRef<Path> + Debug,
    I: IntoIterator<Item = &'glob V> + Debug,
    V: AsRef<str> + 'glob + ?Sized,
{
    let paths = walk_fast(base_dir, patterns)?;

    Ok(paths
        .into_iter()
        .filter(|p| p.is_file())
        .collect::<Vec<_>>())
}
