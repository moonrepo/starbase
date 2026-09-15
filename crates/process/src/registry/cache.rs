use super::process_registry::ProcessRegistry;
use crate::output::Output;
use std::future::Future;
use tokio::sync::watch;

impl ProcessRegistry {
    /// Return the cached output for this key.
    pub async fn get_cached_output(&self, key: &str) -> Option<Output> {
        self.state
            .cache
            .read_async(key, |_, output| output.clone())
            .await
    }

    /// Cache the output for this key. An existing entry is kept, as an
    /// identical command produced it.
    pub async fn cache_output(&self, key: impl Into<String>, output: Output) {
        let _ = self.state.cache.put_async(key.into(), output).await;
    }

    /// Remove the cached output for this key.
    pub async fn remove_cached_output(&self, key: &str) -> Option<Output> {
        self.state
            .cache
            .remove_async(key)
            .await
            .map(|(_, output)| output)
    }

    /// Remove every cached output.
    pub async fn clear_cache(&self) {
        self.state.cache.clear_async().await;
    }

    /// Return the cached output for this key, or run `exec` to produce and
    /// cache it. Concurrent calls with the same key share a single run: the
    /// first caller executes while the others wait for its output. A failed
    /// run is not cached, and the waiters then run `exec` themselves.
    pub async fn exec_cached<F, E>(&self, key: impl Into<String>, exec: F) -> Result<Output, E>
    where
        F: Future<Output = Result<Output, E>>,
    {
        let key = key.into();

        if let Some(output) = self.get_cached_output(&key).await {
            return Ok(output);
        }

        let sender = match self.state.inflight.entry_async(key.clone()).await {
            scc::hash_map::Entry::Occupied(entry) => {
                let mut receiver = entry.get().clone();
                drop(entry);

                match receiver.wait_for(|output| output.is_some()).await {
                    Ok(output) => return Ok(output.clone().unwrap()),
                    // The run failed, so try it ourselves
                    Err(_) => None,
                }
            }
            scc::hash_map::Entry::Vacant(entry) => {
                let (sender, receiver) = watch::channel(None);
                entry.insert_entry(receiver);
                Some(sender)
            }
        };

        let result = exec.await;

        if let Ok(output) = &result {
            self.cache_output(key.clone(), output.clone()).await;

            if let Some(sender) = &sender {
                let _ = sender.send(Some(output.clone()));
            }
        }

        // Only the runner owns the in-flight entry. A failed run drops the
        // sender without a value, which tells the waiters to run themselves.
        if sender.is_some() {
            self.state.inflight.remove_async(&key).await;
        }

        result
    }
}
