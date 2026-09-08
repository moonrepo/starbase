use crate::command::Command;
use crate::output::Output;
use crate::process_error::ProcessError;
use crate::process_registry::ProcessRegistry;
use bytes::Bytes;
use miette::IntoDiagnostic;
use scc::hash_cache::Entry;
use starbase_console::Reporter;
use std::io;
use std::process::Stdio;
use std::time::Instant;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::task::{self, JoinHandle};
use tracing::debug;

impl<R: Reporter> Command<R> {
    async fn internal_exec_capture_output(
        &mut self,
        registry: &ProcessRegistry,
    ) -> miette::Result<Output> {
        let instant = Instant::now();
        let should_pass_stdin = self.should_pass_stdin();
        let mut command = self.create_async_command()?;

        command.stdout(Stdio::piped()).stderr(Stdio::piped());

        if should_pass_stdin {
            command.stdin(Stdio::piped());
        }

        let child = command.spawn().map_err(|error| ProcessError::Capture {
            bin: self.get_bin_name(),
            error: Box::new(error),
        })?;

        let shared_child = registry.add_running(child).await;

        self.pre_log_command(&shared_child);

        let (input_result, result) =
            tokio::join!(self.write_input_to_stdin(&shared_child), async {
                shared_child
                    .wait_with_output()
                    .await
                    .map_err(|error| ProcessError::Capture {
                        bin: self.get_bin_name(),
                        error: Box::new(error),
                    })
            });

        self.post_log_command(&shared_child, instant);

        registry.remove_running(shared_child).await;

        input_result?;
        let output = result?;

        self.handle_nonzero_status(&output, true)?;

        Ok(output)
    }

    /// Spawn the process, write any buffered input, and wait for it to
    /// exit, capturing stdout and stderr. Nothing is streamed to the
    /// console. If [`Self::cache`] is enabled, a prior identical run's
    /// output is reused instead of spawning again.
    pub async fn exec_capture_output(&mut self) -> miette::Result<Output> {
        if self.continuous_pipe {
            return self.exec_capture_continuous_output().await;
        }

        let registry = ProcessRegistry::instance();

        if !self.should_cache_output() {
            return self.internal_exec_capture_output(&registry).await;
        }

        match registry
            .cache
            .entry_async(self.get_output_cache_key("capture"))
            .await
        {
            Entry::Occupied(entry) => self.handle_cached_output(entry.get().clone()),
            Entry::Vacant(entry) => {
                let output = self.internal_exec_capture_output(&registry).await?;

                entry.put_entry(output.clone());

                Ok(output)
            }
        }
    }

    /// A variant of [`Self::exec_capture_output`] that streams buffered
    /// input to the child's stdin as it runs, rather than writing it all
    /// upfront, while capturing stdout and stderr as raw bytes. Used when
    /// [`Self::continuous_pipe`] is enabled. Force-killing stops capture
    /// readers and may truncate unread output.
    pub async fn exec_capture_continuous_output(&mut self) -> miette::Result<Output> {
        let registry = ProcessRegistry::instance();
        let instant = Instant::now();
        let mut command = self.create_async_command()?;

        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let child = command.spawn().map_err(|error| ProcessError::Capture {
            bin: self.get_bin_name(),
            error: Box::new(error),
        })?;

        let shared_child = registry.add_running(child).await;
        let stdin = shared_child.take_stdin().await;
        let stdout = shared_child.take_stdout().await;
        let stderr = shared_child.take_stderr().await;

        self.pre_log_command(&shared_child);

        let items = std::mem::take(&mut self.input);
        let bin_name = self.get_bin_name();

        let stdin_child = shared_child.clone();
        let stdin_handle: JoinHandle<miette::Result<()>> = task::spawn(async move {
            if let Some(mut stdin) = stdin {
                let stopped = stdin_child.wait_till_output_stopped();
                tokio::pin!(stopped);

                for item in items {
                    let write_result = tokio::select! {
                        biased;
                        _ = &mut stopped => break,
                        result = stdin.write_all(item.as_encoded_bytes()) => result,
                    };

                    if let Err(error) = write_result {
                        // The child exited, or closed its stdin, before
                        // consuming all input (e.g. `git hash-object`
                        // erroring on a missing file). Not a failure in
                        // itself: the child's exit status is the outcome.
                        if error.kind() == io::ErrorKind::BrokenPipe {
                            debug!(
                                bin = &bin_name,
                                "Child process closed stdin before all input was written"
                            );

                            break;
                        }

                        return Err(ProcessError::WriteInput {
                            bin: bin_name.clone(),
                            error: Box::new(error),
                        }
                        .into());
                    }
                }

                drop(stdin);
            }

            Ok(())
        });

        let stdout_handle = spawn_capture_bytes(stdout, "stdout", shared_child.clone());
        let stderr_handle = spawn_capture_bytes(stderr, "stderr", shared_child.clone());

        // Attempt to create the child output
        let result = shared_child
            .wait()
            .await
            .map_err(|error| ProcessError::Capture {
                bin: self.get_bin_name(),
                error: Box::new(error),
            });

        self.post_log_command(&shared_child, instant);

        // Keep the child registered until its readers finish, so shutdown
        // can still stop pipes inherited by descendants after the child exits.
        let output_result: miette::Result<Output> = async {
            let exit = result?;

            stdin_handle.await.into_diagnostic()??;

            Ok(Output {
                exit,
                stdout: Bytes::from(stdout_handle.await.into_diagnostic()?),
                stderr: Bytes::from(stderr_handle.await.into_diagnostic()?),
            })
        }
        .await;

        registry.remove_running(shared_child).await;

        let output = output_result?;

        self.handle_nonzero_status(&output, true)?;

        Ok(output)
    }
}

fn spawn_capture_bytes<R>(
    reader: Option<R>,
    label: &'static str,
    child: crate::SharedChild,
) -> JoinHandle<Vec<u8>>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    task::spawn(async move {
        let Some(mut reader) = reader else {
            return vec![];
        };

        let mut output = vec![];
        let mut buffer = [0; 8192];
        let stopped = child.wait_till_output_stopped();

        tokio::pin!(stopped);

        loop {
            let result = tokio::select! {
                biased;
                _ = &mut stopped => break,
                result = reader.read(&mut buffer) => result,
            };

            match result {
                Ok(0) => break,
                Ok(size) => output.extend_from_slice(&buffer[..size]),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    debug!("Failed to read {label} bytes: {error}");
                    break;
                }
            }
        }

        output
    })
}
