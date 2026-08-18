use crate::command::Command;
use crate::output::Output;
use crate::process_error::ProcessError;
use crate::process_registry::ProcessRegistry;
use bytes::Bytes;
use scc::hash_cache::Entry;
use starbase_console::{ConsoleStream, Reporter};
use std::io;
use std::process::Stdio;
use std::time::Instant;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::task::{self, JoinHandle};
use tracing::debug;

impl<R: Reporter> Command<R> {
    async fn internal_exec_stream_and_capture_output(
        &mut self,
        registry: &ProcessRegistry,
    ) -> miette::Result<Output> {
        let instant = Instant::now();
        let mut command = self.create_async_command()?;

        command
            .stdin(if self.should_pass_stdin() {
                Stdio::piped()
            } else {
                Stdio::inherit()
            })
            .stderr(Stdio::piped())
            .stdout(Stdio::piped());

        let mut child = command
            .spawn()
            .map_err(|error| ProcessError::StreamCapture {
                bin: self.get_bin_name(),
                error: Box::new(error),
            })?;

        if self.should_pass_stdin() {
            self.write_input_to_child(&mut child).await?;
        }

        let shared_child = registry.add_running(child).await;

        self.pre_log_command(&shared_child);

        let console = self
            .console
            .as_ref()
            .expect("A console is required when streaming output!");
        let prefix = self.get_prefix().map(|prefix| prefix.to_owned());

        let stderr_handle = spawn_stream_capture_bytes(
            shared_child.take_stderr().await,
            console.stderr(),
            prefix.clone(),
            "stderr",
        );
        let stdout_handle = spawn_stream_capture_bytes(
            shared_child.take_stdout().await,
            console.stdout(),
            prefix,
            "stdout",
        );

        // Wait for the pipes to hit EOF before waiting on the child,
        // otherwise output may be lost
        let captured_stderr = stderr_handle.await.unwrap_or_default();
        let captured_stdout = stdout_handle.await.unwrap_or_default();

        // Attempt to create the child output
        let result = shared_child
            .wait()
            .await
            .map_err(|error| ProcessError::StreamCapture {
                bin: self.get_bin_name(),
                error: Box::new(error),
            });

        self.post_log_command(&shared_child, instant);

        registry.remove_running(shared_child).await;

        let exit = result?;
        let output = Output {
            exit,
            stdout: Bytes::from(captured_stdout),
            stderr: Bytes::from(captured_stderr),
        };

        self.handle_nonzero_status(&output, true)?;

        Ok(output)
    }

    /// Spawn the process, streaming stdout and stderr to the console as
    /// they arrive while also capturing them byte-exact, and wait for it
    /// to exit. Partial lines and carriage-return redraws (progress bars,
    /// spinners) stream live, non-UTF-8 output is preserved, and redraw
    /// frames are collapsed in the captured bytes so a cached replay only
    /// renders the final frame. If [`Self::cache`] is enabled, a prior
    /// identical run's output is returned instead of spawning again, in
    /// which case nothing is streamed to the console.
    pub async fn exec_stream_and_capture_output(&mut self) -> miette::Result<Output> {
        let registry = ProcessRegistry::instance();

        if !self.should_cache_output() {
            return self
                .internal_exec_stream_and_capture_output(&registry)
                .await;
        }

        match registry.cache.entry_async(self.get_cache_key()).await {
            Entry::Occupied(entry) => Ok(entry.get().clone()),
            Entry::Vacant(entry) => {
                let output = self
                    .internal_exec_stream_and_capture_output(&registry)
                    .await?;

                entry.put_entry(output.clone());

                Ok(output)
            }
        }
    }
}

fn spawn_stream_capture_bytes<R>(
    reader: Option<R>,
    stream: ConsoleStream,
    prefix: Option<String>,
    label: &'static str,
) -> JoinHandle<Vec<u8>>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    task::spawn(async move {
        let mut captured = vec![];

        let Some(mut reader) = reader else {
            return captured;
        };

        let mut buf = [0u8; 8192];
        let mut at_line_start = true;

        loop {
            match reader.read(&mut buf).await {
                // EOF
                Ok(0) => break,
                Ok(read) => {
                    let chunk = &buf[..read];

                    // Stream raw bytes to the console so that partial lines
                    // and carriage return based redraws render in real time
                    let _ = stream.write_raw(|out| {
                        match &prefix {
                            Some(prefix) => {
                                for segment in chunk.split_inclusive(|byte| *byte == b'\n') {
                                    if at_line_start {
                                        out.extend_from_slice(prefix.as_bytes());
                                    }

                                    out.extend_from_slice(segment);
                                    at_line_start = segment.ends_with(b"\n");
                                }
                            }
                            None => {
                                out.extend_from_slice(chunk);
                            }
                        };

                        Ok(())
                    });

                    captured.extend_from_slice(chunk);
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                    continue;
                }
                Err(error) => {
                    debug!("Failed to read {label} chunk: {error}");
                    break;
                }
            }
        }

        // Flush any remaining buffered output to ensure all streamed
        // content is visible before the next flow is printed
        let _ = stream.flush();

        collapse_redraw_frames(captured)
    })
}

// Within each line, keep only the content after the last carriage return,
// so that redraw frames (progress bars, spinners) don't replay when the
// captured output is rendered from cache. Trailing `\r\n` line endings
// are not treated as redraws.
fn collapse_redraw_frames(data: Vec<u8>) -> Vec<u8> {
    if !data.contains(&b'\r') {
        return data;
    }

    let mut result = Vec::with_capacity(data.len());

    for line in data.split_inclusive(|byte| *byte == b'\n') {
        let (content, ending): (&[u8], &[u8]) = match line {
            [content @ .., b'\r', b'\n'] => (content, b"\r\n"),
            [content @ .., b'\n'] => (content, b"\n"),
            _ => (line, b""),
        };

        let frame = match content.iter().rposition(|byte| *byte == b'\r') {
            Some(index) => &content[index + 1..],
            None => content,
        };

        result.extend_from_slice(frame);
        result.extend_from_slice(ending);
    }

    result
}
