use crate::glob::{GlobError, GlobSet};
use scc::hash_map::Entry;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use tracing::trace;
use wax::Any;

static INSTANCE: OnceLock<Arc<GlobCache>> = OnceLock::new();

/// A singleton for glob caches.
#[derive(Default)]
pub struct GlobCache {
    cache: scc::HashMap<u64, Vec<PathBuf>>,
    sets: scc::HashMap<u64, Arc<GlobSet<'static>>>,
    prune_sets: scc::HashMap<u64, Option<Arc<Any<'static>>>>,
}

impl GlobCache {
    pub fn instance() -> Arc<GlobCache> {
        Arc::clone(INSTANCE.get_or_init(|| Arc::new(GlobCache::default())))
    }

    pub fn create_key(&self, dir: &Path, globs: &[String]) -> u64 {
        let mut hash = DefaultHasher::default();

        // Both are length prefixed, so the dir and globs can't collide
        dir.as_os_str().hash(&mut hash);
        globs.hash(&mut hash);

        hash.finish()
    }

    pub fn cache<F>(&self, dir: &Path, globs: &[String], op: F) -> Result<Vec<PathBuf>, GlobError>
    where
        F: FnOnce(&Path, &[String]) -> Result<Vec<PathBuf>, GlobError>,
    {
        self.cache_with_key(self.create_key(dir, globs), dir, globs, op)
    }

    // Walk results also depend on the options that filter them
    pub(crate) fn cache_walk<F>(
        &self,
        dir: &Path,
        globs: &[String],
        filters: impl Hash,
        op: F,
    ) -> Result<Vec<PathBuf>, GlobError>
    where
        F: FnOnce(&Path, &[String]) -> Result<Vec<PathBuf>, GlobError>,
    {
        let mut hash = DefaultHasher::default();

        self.create_key(dir, globs).hash(&mut hash);
        filters.hash(&mut hash);

        self.cache_with_key(hash.finish(), dir, globs, op)
    }

    fn cache_with_key<F>(
        &self,
        key: u64,
        dir: &Path,
        globs: &[String],
        op: F,
    ) -> Result<Vec<PathBuf>, GlobError>
    where
        F: FnOnce(&Path, &[String]) -> Result<Vec<PathBuf>, GlobError>,
    {
        // If the cache already exists, allow for parallel reads
        if let Some(value) = self.cache.read_sync(&key, |_, list| list.to_vec()) {
            trace!(
                dir = ?dir,
                globs = ?globs,
                results = value.len(),
                "Reading {} files from cache",
                value.len()
            );

            return Ok(value);
        }

        // Otherwise use an entry so that it creates a lock that avoids parallel writes
        match self.cache.entry_sync(key) {
            Entry::Occupied(entry) => {
                let value = entry.get().to_vec();

                trace!(
                    dir = ?dir,
                    globs = ?globs,
                    "Reading {} files from cache",
                    value.len()
                );

                Ok(value)
            }
            Entry::Vacant(entry) => {
                let value = op(dir, globs)?;

                trace!(
                    dir = ?dir,
                    globs = ?globs,
                    "Writing {} files to cache",
                    value.len()
                );

                entry.insert_entry(value.clone());

                Ok(value)
            }
        }
    }

    pub fn cache_set<F>(
        &self,
        expressions: &[&str],
        negations: &[&str],
        op: F,
    ) -> Result<Arc<GlobSet<'static>>, GlobError>
    where
        F: FnOnce(&[&str], &[&str]) -> Result<GlobSet<'static>, GlobError>,
    {
        let mut hash = DefaultHasher::default();

        // Slices are length prefixed, so expressions and negations can't collide
        expressions.hash(&mut hash);
        negations.hash(&mut hash);

        read_or_insert(&self.sets, hash.finish(), || {
            let set = op(expressions, negations)?;

            trace!(
                expressions = ?expressions,
                negations = ?negations,
                "Writing glob set to cache"
            );

            Ok(Arc::new(set))
        })
    }

    pub(crate) fn cache_prune_set<F>(
        &self,
        negations: &[&str],
        op: F,
    ) -> Result<Option<Arc<Any<'static>>>, GlobError>
    where
        F: FnOnce(&[&str]) -> Result<Option<Any<'static>>, GlobError>,
    {
        let mut hash = DefaultHasher::default();

        negations.hash(&mut hash);

        read_or_insert(&self.prune_sets, hash.finish(), || {
            Ok(op(negations)?.map(Arc::new))
        })
    }

    pub fn reset(&self) {
        self.cache.clear_sync();
        self.sets.clear_sync();
        self.prune_sets.clear_sync();
    }
}

fn read_or_insert<V: Clone + 'static>(
    map: &scc::HashMap<u64, V>,
    key: u64,
    op: impl FnOnce() -> Result<V, GlobError>,
) -> Result<V, GlobError> {
    // If the cache already exists, allow for parallel reads
    if let Some(value) = map.read_sync(&key, |_, value| value.clone()) {
        return Ok(value);
    }

    // Otherwise use an entry so that it creates a lock that avoids parallel writes
    match map.entry_sync(key) {
        Entry::Occupied(entry) => Ok(entry.get().clone()),
        Entry::Vacant(entry) => {
            let value = op()?;

            entry.insert_entry(value.clone());

            Ok(value)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doesnt_cache_failed_operations() {
        let cache = GlobCache::default();
        let dir = PathBuf::from("/root");
        let globs = vec!["**/*".to_string()];

        let result = cache.cache(&dir, &globs, |_, _| {
            Err(GlobError::InvalidPath {
                path: "/fail".into(),
            })
        });

        assert!(result.is_err());

        // A failed operation must not poison the cache for the
        // remainder of the process
        let result = cache
            .cache(&dir, &globs, |_, _| Ok(vec![PathBuf::from("/root/file")]))
            .unwrap();

        assert_eq!(result, vec![PathBuf::from("/root/file")]);
    }

    #[test]
    fn separates_dir_from_globs() {
        let cache = GlobCache::default();
        let key = |dir: &str, globs: &[&str]| {
            cache.create_key(
                Path::new(dir),
                &globs.iter().map(|g| g.to_string()).collect::<Vec<_>>(),
            )
        };

        assert_ne!(key("/root", &["a"]), key("/root:a", &[]));
        assert_ne!(key("/root", &["a:b"]), key("/root", &["a", "b"]));
    }

    #[test]
    fn doesnt_cache_failed_sets() {
        let cache = GlobCache::default();
        let globs = ["**/*"];

        let result = cache.cache_set(&globs, &[], |_, _| {
            Err(GlobError::InvalidPath {
                path: "/fail".into(),
            })
        });

        assert!(result.is_err());

        let a = cache
            .cache_set(&globs, &[], |ex, ng| GlobSet::new_split_owned(ex, ng))
            .unwrap();
        let b = cache
            .cache_set(&globs, &[], |_, _| panic!("should be cached"))
            .unwrap();

        assert!(Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn separates_expressions_from_negations() {
        let cache = GlobCache::default();
        let build = |_: &[&str], _: &[&str]| GlobSet::new_owned(["*"]);

        let a = cache.cache_set(&["a"], &[], build).unwrap();
        let b = cache.cache_set(&[], &["a"], build).unwrap();
        let c = cache.cache_set(&["a:b"], &[], build).unwrap();
        let d = cache.cache_set(&["a", "b"], &[], build).unwrap();

        assert!(!Arc::ptr_eq(&a, &b));
        assert!(!Arc::ptr_eq(&c, &d));
    }
}
