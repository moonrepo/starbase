use crate::glob::{GlobError, GlobSet};
use scc::hash_map::Entry;
use std::hash::{DefaultHasher, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use tracing::trace;

static INSTANCE: OnceLock<Arc<GlobCache>> = OnceLock::new();

/// A singleton for glob caches.
#[derive(Default)]
pub struct GlobCache {
    cache: scc::HashMap<u64, Vec<PathBuf>>,
    sets: scc::HashMap<u64, Arc<GlobSet<'static>>>,
}

impl GlobCache {
    pub fn instance() -> Arc<GlobCache> {
        Arc::clone(INSTANCE.get_or_init(|| Arc::new(GlobCache::default())))
    }

    pub fn create_key(&self, dir: &Path, globs: &[String]) -> u64 {
        let mut hash = DefaultHasher::default();

        hash.write(dir.as_os_str().as_encoded_bytes());

        for glob in globs {
            hash.write(b":");
            hash.write(glob.as_bytes());
        }

        hash.finish()
    }

    pub fn cache<F>(&self, dir: &Path, globs: &[String], op: F) -> Result<Vec<PathBuf>, GlobError>
    where
        F: FnOnce(&Path, &[String]) -> Result<Vec<PathBuf>, GlobError>,
    {
        let key = self.create_key(dir, globs);

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

    pub fn cache_set<F>(&self, globs: &[&str], op: F) -> Result<Arc<GlobSet<'static>>, GlobError>
    where
        F: FnOnce(&[&str]) -> Result<GlobSet<'static>, GlobError>,
    {
        let mut hash = DefaultHasher::default();

        for glob in globs {
            hash.write(b":");
            hash.write(glob.as_bytes());
        }

        let key = hash.finish();

        // If the cache already exists, allow for parallel reads
        if let Some(value) = self.sets.read_sync(&key, |_, set| Arc::clone(set)) {
            return Ok(value);
        }

        // Otherwise use an entry so that it creates a lock that avoids parallel writes
        match self.sets.entry_sync(key) {
            Entry::Occupied(entry) => Ok(Arc::clone(entry.get())),
            Entry::Vacant(entry) => {
                let value = Arc::new(op(globs)?);

                trace!(globs = ?globs, "Writing glob set to cache");

                entry.insert_entry(Arc::clone(&value));

                Ok(value)
            }
        }
    }

    pub fn reset(&self) {
        self.cache.clear_sync();
        self.sets.clear_sync();
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
    fn doesnt_cache_failed_sets() {
        let cache = GlobCache::default();
        let globs = ["**/*"];

        let result = cache.cache_set(&globs, |_| {
            Err(GlobError::InvalidPath {
                path: "/fail".into(),
            })
        });

        assert!(result.is_err());

        let a = cache.cache_set(&globs, |p| GlobSet::new_owned(p)).unwrap();
        let b = cache
            .cache_set(&globs, |_| panic!("should be cached"))
            .unwrap();

        assert!(Arc::ptr_eq(&a, &b));
    }
}
