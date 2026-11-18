use crate::output::Output;
use std::sync::{Arc, LazyLock};

static INSTANCE: LazyLock<OutputCache> = LazyLock::new(|| OutputCache {
    entries: Arc::new(scc::HashCache::new()),
});

/// The output of prior runs, keyed by
/// [`Command::get_cache_key`](crate::Command::get_cache_key), shared by
/// the whole process.
pub(crate) struct OutputCache {
    pub entries: Arc<scc::HashCache<String, Output>>,
}

impl OutputCache {
    pub fn instance() -> &'static OutputCache {
        &INSTANCE
    }

    pub async fn get(&self, key: &str) -> Option<Output> {
        self.entries
            .read_async(key, |_, output| output.clone())
            .await
    }

    pub async fn put(&self, key: String, output: Output) {
        // Another identical command may have completed while this one ran.
        // Keeping the first result is sufficient because both have the same
        // cache identity.
        let _ = self.entries.put_async(key, output).await;
    }
}
