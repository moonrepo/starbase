use crate::fs;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fmt::Debug;
use std::fs::FileType;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, RwLock};
use std::time::Instant;
use tracing::{instrument, trace};
use wax::{
    Any, Program,
    query::{Boundedness, Variance},
    walk::{Entry, FileIterator, LinkBehavior},
};

#[cfg(feature = "glob-cache")]
pub use crate::glob_cache::GlobCache;
pub use crate::glob_error::GlobError;
pub use wax::{self, Glob};

static GLOBAL_NEGATIONS: LazyLock<RwLock<Vec<&'static str>>> =
    LazyLock::new(|| RwLock::new(vec!["**/.{git,svn}/**", "**/.DS_Store", "node_modules/**"]));

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
            expressions: any_glob(ex, "<expressions>")?,
            negations: any_glob(ng, "<negations>")?,
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
            expressions: any_glob(ex, "<expressions>")?,
            negations: any_glob(ng, "<negations>")?,
            enabled: count > 0,
        })
    }

    /// Return true if the path matches the negated patterns.
    pub fn is_excluded<P: AsRef<OsStr>>(&self, path: P) -> bool {
        self.negations.is_match(path.as_ref())
    }

    /// Return true if the path matches the non-negated patterns.
    pub fn is_included<P: AsRef<OsStr>>(&self, path: P) -> bool {
        self.expressions.is_match(path.as_ref())
    }

    /// Return true if the path matches the glob patterns,
    /// while taking into account negated patterns.
    pub fn matches<P: AsRef<OsStr>>(&self, path: P) -> bool {
        if !self.enabled {
            return false;
        }

        let path = path.as_ref();

        if self.is_excluded(path) {
            return false;
        }

        self.is_included(path)
    }
}

/// Parse and create a [`Glob`] instance from the borrowed string pattern.
/// If parsing fails, a [`GlobError`] is returned.
#[inline]
pub fn create_glob(pattern: &str) -> Result<Glob<'_>, GlobError> {
    Glob::new(pattern).map_err(|error| GlobError::Create {
        glob: pattern.to_owned(),
        error: Box::new(error),
    })
}

/// Combine a list of globs into a single [`Any`] matcher, returning a
/// [`GlobError`] instead of panicking if the combination is invalid.
#[inline]
pub fn any_glob<'a, I>(globs: I, desc: &str) -> Result<Any<'a>, GlobError>
where
    I: IntoIterator<Item = Glob<'a>>,
{
    wax::any(globs).map_err(|error| GlobError::Create {
        glob: desc.to_owned(),
        error: Box::new(error),
    })
}

/// Return true if the provided string looks like a glob pattern.
/// This is not exhaustive and may be inaccurate.
#[inline]
pub fn is_glob<T: AsRef<str> + Debug>(value: T) -> bool {
    let value = value.as_ref();

    if value.contains("**") || value.starts_with('!') {
        return true;
    }

    let is_escaped =
        |index: usize| index > 0 && value.as_bytes().get(index - 1).is_some_and(|b| *b == b'\\');

    for single in ['*', '?'] {
        if value
            .match_indices(single)
            .any(|(index, _)| !is_escaped(index))
        {
            return true;
        }
    }

    for (open, close) in [('{', '}'), ('[', ']')] {
        if value.contains(close)
            && value
                .match_indices(open)
                .any(|(index, _)| !is_escaped(index))
        {
            return true;
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
        Some(p) => {
            if p.contains('\\') {
                Ok(p.replace('\\', "/"))
            } else {
                Ok(p.to_owned())
            }
        }
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
    walk_internal(base_dir, patterns, false)
}

fn walk_internal<'glob, P, I, V>(
    base_dir: P,
    patterns: I,
    files_only: bool,
) -> Result<Vec<PathBuf>, GlobError>
where
    P: AsRef<Path> + Debug,
    I: IntoIterator<Item = &'glob V> + Debug,
    V: AsRef<str> + 'glob + ?Sized,
{
    let base_dir = base_dir.as_ref();
    let instant = Instant::now();
    let mut paths = vec![];

    trace!(dir = ?base_dir, globs = ?patterns, "Finding files");

    let (expressions, mut negations) = split_patterns(patterns);
    negations.extend(GLOBAL_NEGATIONS.read().unwrap().iter());

    // An invalid negation pattern must surface as an error rather than panic.
    let negations_desc = negations.join(", ");
    let negations_set = wax::any(negations).map_err(|error| GlobError::Create {
        glob: negations_desc.clone(),
        error: Box::new(error),
    })?;

    for expression in expressions {
        let walker = create_glob(expression)?
            .walk_with_behavior(base_dir, LinkBehavior::ReadFile)
            .not(negations_set.clone())
            .map_err(|error| GlobError::Create {
                glob: negations_desc.clone(),
                error: Box::new(error),
            })?;

        for entry in walker {
            match entry {
                Ok(e) => {
                    // Reuse the file type already resolved during the walk
                    // instead of re-stat-ing every returned path afterwards.
                    if files_only && !e.file_type().is_file() {
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

    trace!(dir = ?base_dir, "Found {} in {:?}", paths.len(), instant.elapsed());

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
    walk_internal(base_dir, patterns, true)
}

/// Options to customize walking behavior.
#[derive(Debug, Default)]
pub struct GlobWalkOptions {
    pub cache: bool,
    pub ignore_dot_dirs: bool,
    pub ignore_dot_files: bool,
    pub log_results: bool,
    pub only_dirs: bool,
    pub only_files: bool,
}

impl GlobWalkOptions {
    /// Cache the results globally.
    pub fn cache(mut self) -> Self {
        self.cache = true;
        self
    }

    /// Only return directories.
    pub fn dirs(mut self) -> Self {
        self.only_dirs = true;
        self
    }

    /// Only return files.
    pub fn files(mut self) -> Self {
        self.only_files = true;
        self
    }

    /// Control directories that start with a `.`.
    pub fn dot_dirs(mut self, ignore: bool) -> Self {
        self.ignore_dot_dirs = ignore;
        self
    }

    /// Control files that start with a `.`.
    pub fn dot_files(mut self, ignore: bool) -> Self {
        self.ignore_dot_files = ignore;
        self
    }

    /// Log the results.
    pub fn log_results(mut self) -> Self {
        self.log_results = true;
        self
    }
}

/// Walk the file system starting from the provided directory, and return all files and directories
/// that match the provided glob patterns.
#[inline]
pub fn walk_fast<'glob, P, I, V>(base_dir: P, patterns: I) -> Result<Vec<PathBuf>, GlobError>
where
    P: AsRef<Path> + Debug,
    I: IntoIterator<Item = &'glob V> + Debug,
    V: AsRef<str> + 'glob + ?Sized,
{
    walk_fast_with_options(base_dir, patterns, GlobWalkOptions::default())
}

/// Walk the file system starting from the provided directory, and return all files and directories
/// that match the provided glob patterns, and customize further with the provided options.
#[inline]
#[instrument]
pub fn walk_fast_with_options<'glob, P, I, V>(
    base_dir: P,
    patterns: I,
    options: GlobWalkOptions,
) -> Result<Vec<PathBuf>, GlobError>
where
    P: AsRef<Path> + Debug,
    I: IntoIterator<Item = &'glob V> + Debug,
    V: AsRef<str> + 'glob + ?Sized,
{
    let mut paths = vec![];

    for (dir, mut patterns) in partition_patterns(base_dir, patterns) {
        patterns.sort();

        // Only run if the feature is enabled
        #[cfg(feature = "glob-cache")]
        if options.cache && !crate::envx::is_test() {
            paths.extend(
                GlobCache::instance()
                    .cache(&dir, &patterns, |d, p| internal_walk(d, p, &options))?,
            );

            continue;
        }

        paths.extend(internal_walk(&dir, &patterns, &options)?);
    }

    Ok(paths)
}

fn internal_walk(
    dir: &Path,
    patterns: &[String],
    options: &GlobWalkOptions,
) -> Result<Vec<PathBuf>, GlobError> {
    trace!(dir = ?dir, globs = ?patterns, "Finding files");

    let instant = Instant::now();
    let max_depth = max_traversal_depth(patterns)?;
    let globset = GlobSet::new(patterns)?;
    let mut paths = vec![];

    let mut add_path =
        |path: PathBuf, file_type: FileType, base_dir: &Path, globset: &GlobSet<'_>| {
            if file_type.is_file()
                && (options.only_dirs || options.ignore_dot_files && is_hidden_dot(&path))
            {
                return;
            }

            if file_type.is_dir()
                && (options.only_files || options.ignore_dot_dirs && is_hidden_dot(&path))
            {
                return;
            }

            if let Ok(suffix) = path.strip_prefix(base_dir)
                && globset.matches(suffix)
            {
                paths.push(path);
            }
        };

    if max_depth.is_none_or(|depth| depth > 1) {
        let ignore_dot_dirs = options.ignore_dot_dirs;
        let mut walk = jwalk::WalkDir::new(dir)
            .follow_links(false)
            .skip_hidden(false);

        if let Some(max_depth) = max_depth {
            walk = walk.max_depth(max_depth);
        }

        for entry in walk
            .process_read_dir(move |depth, path, _state, children| {
                // Only ignore nested hidden dirs, but do not ignore
                // if the root dir is hidden, as globs resolve from it
                if ignore_dot_dirs && depth.is_some_and(|d| d > 0) && is_hidden_dot(path) {
                    children.retain(|_| false);
                }
            })
            .into_iter()
            .flatten()
        {
            add_path(entry.path(), entry.file_type(), dir, &globset);
        }
    } else {
        for entry in fs::read_dir(dir)? {
            if let Ok(file_type) = entry.file_type() {
                add_path(entry.path(), file_type, dir, &globset);
            }
        }
    }

    trace!(
        dir = ?dir,
        results = ?if options.log_results {
            Some(&paths)
        } else {
            None
        },
        "Found {} in {:?}",
        paths.len(),
        instant.elapsed(),
    );

    Ok(paths)
}

/// Partition a list of patterns and a base directory into buckets, keyed by the common
/// parent directory. This helps to alleviate over-globbing on large directories.
pub fn partition_patterns<'glob, P, I, V>(
    base_dir: P,
    patterns: I,
) -> BTreeMap<PathBuf, Vec<String>>
where
    P: AsRef<Path> + Debug,
    I: IntoIterator<Item = &'glob V> + Debug,
    V: AsRef<str> + 'glob + ?Sized,
{
    let base_dir = base_dir.as_ref();
    let mut partitions = BTreeMap::new();

    // Sort patterns from smallest to longest glob,
    // so that we can create the necessary buckets correctly
    let mut patterns = patterns.into_iter().map(|p| p.as_ref()).collect::<Vec<_>>();
    patterns.sort_by_key(|a| a.len());

    // Negations restrict *every* bucket that could contain a match, not just the
    // one their literal prefix points at. Collect them up front (relative to the
    // base dir, without the leading `!`) and apply them to each bucket below,
    // re-relativized to that bucket. Routing a negation into its own bucket --
    // as this used to -- silently drops it from the broader buckets that its
    // positive counterpart actually walks.
    let negations = patterns
        .iter()
        .filter_map(|pattern| {
            pattern
                .strip_prefix('!')
                .map(|negation| negation.trim_start_matches("./").to_owned())
        })
        .collect::<Vec<_>>();

    for pattern in patterns {
        // Negations are handled separately below.
        if pattern.starts_with('!') {
            continue;
        }

        let mut dir = base_dir.to_path_buf();
        let mut glob_parts = vec![];
        let mut found = false;

        let parts = pattern
            .trim_start_matches("./")
            .split('/')
            .collect::<Vec<_>>();
        let last_index = parts.len() - 1;

        for (index, part) in parts.into_iter().enumerate() {
            if part.is_empty() {
                continue;
            }

            if found || index == last_index || is_glob(part) {
                glob_parts.push(part);
                found = true;
            } else {
                dir = dir.join(part);

                if partitions.contains_key(&dir) {
                    found = true;
                }
            }
        }

        partitions
            .entry(dir)
            .or_insert(vec![])
            .push(glob_parts.join("/"));
    }

    if !negations.is_empty() {
        for (dir, value) in partitions.iter_mut() {
            for negation in &negations {
                if let Some(rel) = relativize_negation(base_dir, dir, negation) {
                    value.push(format!("!{rel}"));
                }
            }
        }
    }

    partitions
}

/// Re-express a negation (relative to `base_dir`, without the leading `!`) so it
/// applies within a specific bucket directory. Returns [`None`] when the negation
/// cannot match anything inside that bucket, so it can be skipped there.
fn relativize_negation(base_dir: &Path, bucket_dir: &Path, negation: &str) -> Option<String> {
    // Floating negations (leading `**`) match at any depth, so they apply to
    // every bucket unchanged.
    if negation.starts_with("**") {
        return Some(negation.to_owned());
    }

    // The base bucket sees paths exactly as authored.
    let Ok(bucket_rel) = bucket_dir.strip_prefix(base_dir) else {
        return Some(negation.to_owned());
    };

    if bucket_rel.as_os_str().is_empty() {
        return Some(negation.to_owned());
    }

    // A non-UTF-8 bucket path can't be compared against string globs; keep the
    // negation as-is so we err toward excluding rather than leaking files.
    let Some(bucket_rel) = bucket_rel.to_str() else {
        return Some(negation.to_owned());
    };
    let bucket_rel = bucket_rel.replace('\\', "/");

    let mut neg_parts = negation.split('/');

    for bucket_part in bucket_rel.split('/') {
        match neg_parts.next() {
            // `**` spans this bucket segment and any deeper ones, so the
            // remainder (including the `**`) applies within the bucket.
            Some("**") => {
                let rest = std::iter::once("**")
                    .chain(neg_parts)
                    .collect::<Vec<_>>()
                    .join("/");

                return Some(rest);
            }
            // Consume a literal or wildcard segment that matches this bucket
            // segment and keep descending.
            Some(neg_part) if neg_part == bucket_part || segment_matches(neg_part, bucket_part) => {
                continue;
            }
            // The negation diverges from this bucket's path, so it can't match
            // anything inside it.
            Some(_) => return None,
            // The negation is a proper prefix of the bucket path and did not end
            // in `**`, so it targets an ancestor entry, not this bucket's files.
            None => return None,
        }
    }

    let rest = neg_parts.collect::<Vec<_>>();

    if rest.is_empty() {
        // The negation names the bucket directory itself; a glob without a
        // trailing wildcard doesn't match the directory's contents.
        None
    } else {
        Some(rest.join("/"))
    }
}

fn segment_matches(pattern: &str, segment: &str) -> bool {
    is_glob(pattern) && create_glob(pattern).is_ok_and(|glob| glob.is_match(Path::new(segment)))
}

fn max_traversal_depth(patterns: &[String]) -> Result<Option<usize>, GlobError> {
    let mut max_depth = 1;

    if patterns
        .iter()
        .filter(|pattern| !pattern.starts_with('!'))
        .all(|pattern| !pattern.contains('/') && !pattern.contains("**"))
    {
        return Ok(Some(max_depth));
    }

    for pattern in patterns {
        if pattern.starts_with('!') {
            continue;
        }

        match create_glob(pattern)?.depth() {
            Variance::Invariant(depth) => {
                max_depth = max_depth.max(depth);
            }
            Variance::Variant(range) => match range.upper() {
                Boundedness::Bounded(depth) => {
                    max_depth = max_depth.max(depth.get());
                }
                Boundedness::Unbounded => {
                    return Ok(None);
                }
            },
        }
    }

    Ok(Some(max_depth))
}

fn is_hidden_dot(path: &Path) -> bool {
    path.file_name()
        .and_then(|file| file.to_str())
        .is_some_and(|name| name.starts_with('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patterns(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn detects_bounded_traversal_depth() {
        assert_eq!(max_traversal_depth(&patterns(&["*"])).unwrap(), Some(1));
        assert_eq!(
            max_traversal_depth(&patterns(&["*/moon.yml"])).unwrap(),
            Some(2)
        );
        assert_eq!(
            max_traversal_depth(&patterns(&["a/*/file.txt"])).unwrap(),
            Some(3)
        );
        assert_eq!(
            max_traversal_depth(&patterns(&["*/moon.yml", "a/*/file.txt"])).unwrap(),
            Some(3)
        );
    }

    #[test]
    fn detects_unbounded_traversal_depth() {
        assert_eq!(max_traversal_depth(&patterns(&["**/*"])).unwrap(), None);
        assert_eq!(
            max_traversal_depth(&patterns(&["*/moon.yml", "**/*.rs"])).unwrap(),
            None
        );
    }

    #[test]
    fn ignores_negations_for_traversal_depth() {
        assert_eq!(
            max_traversal_depth(&patterns(&["*", "!**/target/**"])).unwrap(),
            Some(1)
        );
    }
}
