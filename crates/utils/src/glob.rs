use miette::Diagnostic;
use once_cell::sync::Lazy;
use starbase_styles::{Style, Stylize};
use std::sync::RwLock;
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};
use thiserror::Error;
use wax::{Any, BuildError, LinkBehavior, Negation, Pattern};

pub use wax::{self, Glob};

#[derive(Error, Diagnostic, Debug)]
pub enum GlobError {
    #[diagnostic(code(glob::create))]
    #[error("Failed to create glob from pattern {}", .glob.style(Style::File))]
    Create {
        glob: String,
        #[source]
        error: BuildError<'static>,
    },

    #[diagnostic(code(glob::invalid_path))]
    #[error("Failed to normalize glob path {}", .path.style(Style::Path))]
    InvalidPath { path: PathBuf },
}

static GLOBAL_NEGATIONS: Lazy<RwLock<Vec<&'static str>>> = Lazy::new(|| {
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

pub struct GlobSet<'glob> {
    expressions: Any<'glob>,
    negations: Any<'glob>,
    enabled: bool,
}

impl<'glob> GlobSet<'glob> {
    /// Create a new glob set from the list of patterns. Negated patterns must start with `!`.
    pub fn new<I, V>(patterns: I) -> Result<Self, GlobError>
    where
        I: IntoIterator<Item = &'glob V>,
        V: AsRef<str> + 'glob + ?Sized,
    {
        let (expressions, negations) = split_patterns(patterns);

        GlobSet::new_split(expressions, negations)
    }

    /// Create a new glob set with explicitly separate expressions and negations.
    /// Negated patterns must not start with `!`.
    pub fn new_split<I1, V1, I2, V2>(expressions: I1, negations: I2) -> Result<Self, GlobError>
    where
        I1: IntoIterator<Item = &'glob V1>,
        V1: AsRef<str> + 'glob + ?Sized,
        I2: IntoIterator<Item = &'glob V2>,
        V2: AsRef<str> + 'glob + ?Sized,
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
            expressions: wax::any::<Glob<'glob>, _>(ex).unwrap(),
            negations: wax::any::<Glob<'glob>, _>(ng).unwrap(),
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

/// Parse and create a [Glob] instance from the borrowed string pattern.
/// If parsing fails, a [GlobError] is returned.
#[inline]
pub fn create_glob(pattern: &str) -> Result<Glob<'_>, GlobError> {
    Glob::new(pattern).map_err(|error| GlobError::Create {
        glob: pattern.to_owned(),
        error: error.into_owned(),
    })
}

/// Return true if the provided string looks like a glob pattern.
/// This is not exhaustive and may be inaccurate.
#[inline]
pub fn is_glob<T: AsRef<str>>(value: T) -> bool {
    let value = value.as_ref();
    let single_values = vec!['*', '?', '!'];
    let paired_values = vec![('{', '}'), ('[', ']')];
    let mut bytes = value.bytes();
    let mut is_escaped = |index: usize| {
        if index == 0 {
            return false;
        }

        bytes.nth(index - 1).unwrap_or(b' ') == b'\\'
    };

    if value.contains("**") {
        return true;
    }

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
/// invalid UTF-8 characters, a [GlobError] is returned.
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
pub fn split_patterns<'glob, I, V>(patterns: I) -> (Vec<&'glob str>, Vec<&'glob str>)
where
    I: IntoIterator<Item = &'glob V>,
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

        if negate {
            negations.push(value);
        } else {
            expressions.push(value);
        }
    }

    (expressions, negations)
}

/// Walk the file system starting from the provided directory, and return all files and directories
/// that match the provided glob patterns. Use [walk_files] if you only want to return files.
#[inline]
pub fn walk<'glob, P, I, V>(base_dir: P, patterns: I) -> Result<Vec<PathBuf>, GlobError>
where
    P: AsRef<Path>,
    I: IntoIterator<Item = &'glob V>,
    V: AsRef<str> + 'glob + ?Sized,
{
    let (expressions, mut negations) = split_patterns(patterns);
    negations.extend(GLOBAL_NEGATIONS.read().unwrap().iter());

    let negation = Negation::try_from_patterns(negations).unwrap();
    let mut paths = vec![];

    for expression in expressions {
        for entry in
            create_glob(expression)?.walk_with_behavior(base_dir.as_ref(), LinkBehavior::ReadFile)
        {
            match entry {
                Ok(e) => {
                    // Filter out negated results
                    if negation.target(&e).is_some() {
                        continue;
                    }

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
/// that match the provided glob patterns. Use [walk] if you need to get directories as well.
#[inline]
pub fn walk_files<'glob, P, I, V>(base_dir: P, patterns: I) -> Result<Vec<PathBuf>, GlobError>
where
    P: AsRef<Path>,
    I: IntoIterator<Item = &'glob V>,
    V: AsRef<str> + 'glob + ?Sized,
{
    let paths = walk(base_dir, patterns)?;

    Ok(paths
        .into_iter()
        .filter(|p| p.is_file())
        .collect::<Vec<_>>())
}
