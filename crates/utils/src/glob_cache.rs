use crate::glob::GlobError;
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

    pub fn reset(&self) {
        self.cache.clear_sync();
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
}
