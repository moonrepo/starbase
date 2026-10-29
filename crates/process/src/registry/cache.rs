//! Output cache keys, expiration, and resource bounds.

use super::RegistryOptions;
use super::lifecycle::State;
use crate::Output;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Clone, Eq, PartialEq)]
pub(super) struct CacheKey {
    pub(super) program: OsString,
    pub(super) args: Vec<OsString>,
    pub(super) cwd: PathBuf,
    pub(super) env: BTreeMap<OsString, OsString>,
    pub(super) input: Vec<u8>,
}

pub(super) struct CacheEntry {
    key: CacheKey,
    output: Output,
    inserted: Instant,
}

impl State {
    pub(super) fn cached(&mut self, key: &CacheKey, options: &RegistryOptions) -> Option<Output> {
        self.cache
            .retain(|entry| entry.inserted.elapsed() < options.cache_ttl);
        self.cache_bytes = self
            .cache
            .iter()
            .map(|entry| output_size(&entry.output))
            .sum();
        let index = self.cache.iter().position(|entry| &entry.key == key)?;
        let entry = self.cache.remove(index)?;
        let output = entry.output.clone();
        self.cache.push_back(entry);
        Some(output)
    }

    pub(super) fn cache(
        &mut self,
        key: CacheKey,
        epoch: u64,
        output: &Output,
        options: &RegistryOptions,
    ) {
        let size = output_size(output);
        if epoch != self.cache_epoch
            || !output.success()
            || options.cache_capacity == 0
            || size > options.cache_max_bytes
            || options.cache_ttl.is_zero()
        {
            return;
        }
        self.cache
            .retain(|entry| entry.key != key && entry.inserted.elapsed() < options.cache_ttl);
        self.cache_bytes = self
            .cache
            .iter()
            .map(|entry| output_size(&entry.output))
            .sum();
        while self.cache.len() >= options.cache_capacity
            || self.cache_bytes > options.cache_max_bytes - size
        {
            let Some(entry) = self.cache.pop_front() else {
                break;
            };
            self.cache_bytes -= output_size(&entry.output);
        }
        self.cache_bytes += size;
        self.cache.push_back(CacheEntry {
            key,
            output: output.clone(),
            inserted: Instant::now(),
        });
    }
}

fn output_size(output: &Output) -> usize {
    output.stdout.len().saturating_add(output.stderr.len())
}
