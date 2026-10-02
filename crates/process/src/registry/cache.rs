use super::process_registry::ProcessRegistry;
use super::state::Inflight;
use crate::output::Output;
use std::future::Future;
use tokio::sync::watch;

/// Owns the in-flight entry of a run, and removes it when dropped. A run
/// can be dropped at any await, such as by a timeout, and would otherwise
/// leave a dead entry behind that stops later calls from sharing a run.
struct InflightGuard<'a> {
    inflight: &'a Inflight,
    key: &'a str,
    sender: watch::Sender<Option<Output>>,
}

impl Drop for InflightGuard<'_> {
    fn drop(&mut self) {
        // Removed before the sender is dropped, so a waiter that is woken
        // by a run without output no longer finds this entry
        self.inflight.remove_sync(self.key);
    }
}

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
    /// first caller executes while the others wait for its output. A run
    /// that fails or is cancelled is not cached, and the callers that were
    /// waiting on it then run `exec` themselves.
    pub async fn exec_cached<F, E>(&self, key: impl Into<String>, exec: F) -> Result<Output, E>
    where
        F: Future<Output = Result<Output, E>>,
    {
        let key = key.into();

        if let Some(output) = self.get_cached_output(&key).await {
            return Ok(output);
        }

        // Only the first caller owns the in-flight entry, through the guard
        let guard = match self.state.inflight.entry_async(key.clone()).await {
            scc::hash_map::Entry::Occupied(entry) => {
                let mut receiver = entry.get().clone();
                drop(entry);

                match receiver.wait_for(|output| output.is_some()).await {
                    Ok(output) => return Ok(output.clone().unwrap()),
                    // The run failed or was cancelled, so try it ourselves
                    Err(_) => None,
                }
            }
            scc::hash_map::Entry::Vacant(entry) => {
                let (sender, receiver) = watch::channel(None);
                entry.insert_entry(receiver);

                Some(InflightGuard {
                    inflight: &self.state.inflight,
                    key: &key,
                    sender,
                })
            }
        };

        // Returning early, or being dropped here, drops the guard without
        // sending an output, which tells the waiters to run themselves
        let output = exec.await?;

        self.cache_output(key.clone(), output.clone()).await;

        if let Some(guard) = &guard {
            let _ = guard.sender.send(Some(output.clone()));
        }

        Ok(output)
    }
}
