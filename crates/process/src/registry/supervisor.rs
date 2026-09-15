//! Supervise child exit, concurrent I/O, and completion cleanup.

use super::cache::CacheKey;
use super::lifecycle::Core;
use super::process::Process;
#[cfg(unix)]
use super::unix::wait_unreaped;
use super::{RegistryEvent, lock};
use crate::{Output, SharedChild};
use bytes::Bytes;
use std::io;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

pub(super) struct Supervisor {
    pub(super) core: Arc<Core>,
    pub(super) process: Arc<Process>,
    pub(super) finished: bool,
}

impl Supervisor {
    fn finish(&mut self, result: io::Result<Output>, cache: Option<(CacheKey, u64)>) {
        let result = result.map_err(Arc::new);
        let mut state = lock(&self.core.state);
        if let (Ok(output), Some((key, epoch))) = (&result, cache) {
            state.cache(key, epoch, output, &self.core.options);
        }
        state.running.remove(&self.process.id);
        let event = match &result {
            Ok(output) => RegistryEvent::Completed {
                id: self.process.id,
                exit: output.exit.clone(),
            },
            Err(error) => RegistryEvent::Failed {
                id: self.process.id,
                error: error.clone(),
            },
        };
        self.process.result.send_replace(Some(result));
        let _ = self.core.events.send(event);
        self.core
            .changes
            .send_modify(|version| *version = version.wrapping_add(1));
        self.finished = true;
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        if !self.finished {
            if let Err(error) = self.process.finish_tree() {
                tracing::warn!(%error, "Failed to clean up cancelled process supervisor");
            }
            self.process.child.stop_output();
            self.finish(
                Err(io::Error::other("process supervisor was cancelled")),
                None,
            );
        }
    }
}

pub(super) async fn supervise(
    mut guard: Supervisor,
    input: Vec<u8>,
    cache: Option<(CacheKey, u64)>,
    #[cfg(unix)] mut child_signals: tokio::signal::unix::Signal,
) {
    let process = guard.process.clone();
    let child = &process.child;
    let mut stdin = child.take_stdin().await;
    let stdout = child.take_stdout().await;
    let stderr = child.take_stderr().await;
    let limit = guard.core.options.max_output_bytes;
    let result = {
        let io = async {
            let write = async {
                if let Some(mut stdin) = stdin.take() {
                    tokio::select! {
                        _ = child.wait_till_output_stopped() => {},
                        result = stdin.write_all(&input) => {
                            if let Err(error) = result && error.kind() != io::ErrorKind::BrokenPipe {
                                return Err(error);
                            }
                        }
                    }
                }
                Ok::<_, io::Error>(())
            };
            let (_, stdout, stderr) = tokio::try_join!(
                write,
                read_output(stdout, child, limit),
                read_output(stderr, child, limit)
            )?;
            Ok::<_, io::Error>((stdout, stderr))
        };
        let wait = async {
            #[cfg(unix)]
            wait_unreaped(&process, &mut child_signals).await?;
            #[cfg(unix)]
            process.finish_tree()?;
            let exit = child.wait().await?;
            #[cfg(windows)]
            process.finish_tree()?;
            Ok::<_, io::Error>(exit)
        };
        tokio::pin!(io, wait);
        tokio::select! {
            result = &mut io => match result {
                Ok((stdout, stderr)) => wait.await.map(|exit| Output { exit, stdout, stderr }),
                Err(error) => Err(error),
            },
            result = &mut wait => match result {
                Ok(exit) => match tokio::time::timeout(guard.core.options.output_drain_timeout, &mut io).await {
                    Ok(result) => result.map(|(stdout, stderr)| Output { exit, stdout, stderr }),
                    Err(_) => Err(io::Error::new(io::ErrorKind::TimedOut, "process output pipes did not close after exit")),
                },
                Err(error) => Err(error),
            },
        }
    };
    if result.is_err() {
        if let Err(error) = process.finish_tree() {
            tracing::warn!(%error, "Failed to terminate process tree after supervision error");
        }
        child.stop_output();
        // Always reap, including on capture errors and output limits.
        if let Err(error) = child.wait().await {
            tracing::warn!(%error, "Failed to reap process after supervision error");
        }
    }
    guard.finish(result, cache);
}

async fn read_output<R: AsyncRead + Unpin>(
    reader: Option<R>,
    child: &SharedChild,
    limit: usize,
) -> io::Result<Bytes> {
    let Some(mut reader) = reader else {
        return Ok(Bytes::new());
    };
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let size = tokio::select! {
            biased;
            _ = child.wait_till_output_stopped() => break,
            result = reader.read(&mut buffer) => result?,
        };
        if size == 0 {
            break;
        }
        if size > limit.saturating_sub(bytes.len()) {
            return Err(io::Error::other(
                "process output exceeded configured byte limit",
            ));
        }
        bytes.extend_from_slice(&buffer[..size]);
    }
    Ok(Bytes::from(bytes))
}
