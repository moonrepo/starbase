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

static GLOBAL_NEGATIONS: Lazy<RwLock<Vec<&'static str>>> =
    Lazy::new(|| RwLock::new(vec!["**/.{git,svn}/**", "**/node_modules/**"]));

pub fn add_global_negations<I>(patterns: I)
where
    I: IntoIterator<Item = &'static str>,
{
    let mut negations = GLOBAL_NEGATIONS.write().unwrap();
    negations.extend(patterns);
}

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
    pub fn new<I, V>(patterns: I) -> Result<Self, GlobError>
    where
        I: IntoIterator<Item = &'glob V>,
        V: AsRef<str> + 'glob + ?Sized,
    {
        let (expressions, negations) = split_patterns(patterns);

        GlobSet::new_split(expressions, negations)
    }

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

    pub fn is_negated<P: AsRef<OsStr>>(&self, path: P) -> bool {
        self.negations.is_match(path.as_ref())
    }

    pub fn is_match<P: AsRef<OsStr>>(&self, path: P) -> bool {
        self.expressions.is_match(path.as_ref())
    }

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

#[inline]
pub fn create_glob(pattern: &str) -> Result<Glob<'_>, GlobError> {
    Glob::new(pattern).map_err(|error| GlobError::Create {
        glob: pattern.to_owned(),
        error: error.into_owned(),
    })
}

// This is not very exhaustive and may be inaccurate.
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

/// Wax currently doesn't support negated globs (starts with !),
/// so we must extract them manually.
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

#[cfg(test)]
mod tests {
    use super::*;

    mod globset {
        use super::*;

        #[test]
        fn doesnt_match_when_empty() {
            let list: Vec<String> = vec![];
            let set = GlobSet::new(&list).unwrap();

            assert!(!set.matches("file.ts"));

            // Testing types
            let list: Vec<&str> = vec![];
            let set = GlobSet::new(list).unwrap();

            assert!(!set.matches("file.ts"));
        }

        #[test]
        fn matches_explicit() {
            let set = GlobSet::new(["source"]).unwrap();

            assert!(set.matches("source"));
            assert!(!set.matches("source.ts"));
        }

        #[test]
        fn matches_exprs() {
            let set = GlobSet::new(["files/*.ts"]).unwrap();

            assert!(set.matches("files/index.ts"));
            assert!(set.matches("files/test.ts"));
            assert!(!set.matches("index.ts"));
            assert!(!set.matches("files/index.js"));
            assert!(!set.matches("files/dir/index.ts"));
        }

        #[test]
        fn doesnt_match_negations() {
            let set = GlobSet::new(["files/*", "!**/*.ts"]).unwrap();

            assert!(set.matches("files/test.js"));
            assert!(set.matches("files/test.go"));
            assert!(!set.matches("files/test.ts"));
        }

        #[test]
        fn doesnt_match_negations_using_split() {
            let set = GlobSet::new_split(["files/*"], ["**/*.ts"]).unwrap();

            assert!(set.matches("files/test.js"));
            assert!(set.matches("files/test.go"));
            assert!(!set.matches("files/test.ts"));
        }

        #[test]
        fn doesnt_match_global_negations() {
            let set = GlobSet::new(["files/**/*"]).unwrap();

            assert!(set.matches("files/test.js"));
            assert!(!set.matches("files/node_modules/test.js"));
            assert!(!set.matches("files/.git/cache"));
        }
    }

    mod is_glob {
        use super::*;

        #[test]
        fn returns_true_when_a_glob() {
            assert!(is_glob("**"));
            assert!(is_glob("**/src/*"));
            assert!(is_glob("src/**"));
            assert!(is_glob("*.ts"));
            assert!(is_glob("file.*"));
            assert!(is_glob("file.{js,ts}"));
            assert!(is_glob("file.[jstx]"));
            assert!(is_glob("file.tsx?"));
        }

        #[test]
        fn returns_false_when_not_glob() {
            assert!(!is_glob("dir"));
            assert!(!is_glob("file.rs"));
            assert!(!is_glob("dir/file.ts"));
            assert!(!is_glob("dir/dir/file_test.rs"));
            assert!(!is_glob("dir/dirDir/file-ts.js"));
        }

        #[test]
        fn returns_false_when_escaped_glob() {
            assert!(!is_glob("\\*.rs"));
            assert!(!is_glob("file\\?.js"));
            assert!(!is_glob("folder-\\[id\\]"));
        }
    }

    mod split_patterns {
        use super::*;

        #[test]
        fn splits_all_patterns() {
            assert_eq!(
                split_patterns(["*.file", "!neg1.*", "/*.file2", "/!neg2.*", "!/neg3.*"]),
                (
                    vec!["*.file", "*.file2"],
                    vec!["neg1.*", "neg2.*", "neg3.*"]
                )
            );
        }
    }
}
