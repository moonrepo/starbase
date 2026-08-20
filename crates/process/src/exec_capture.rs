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
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio::task::{self, JoinHandle};
use tracing::debug;

impl<R: Reporter> Command<R> {
    async fn internal_exec_capture_output(
        &mut self,
        registry: &ProcessRegistry,
    ) -> miette::Result<Output> {
        let instant = Instant::now();
        let mut command = self.create_async_command()?;

        let child = if self.should_pass_stdin() {
            command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            let mut child = command.spawn().map_err(|error| ProcessError::Capture {
                bin: self.get_bin_name(),
                error: Box::new(error),
            })?;

            self.write_input_to_child(&mut child).await?;

            child
        } else {
            command.stdout(Stdio::piped()).stderr(Stdio::piped());

            command.spawn().map_err(|error| ProcessError::Capture {
                bin: self.get_bin_name(),
                error: Box::new(error),
            })?
        };

        let shared_child = registry.add_running(child).await;

        self.pre_log_command(&shared_child);

        let result = shared_child
            .wait_with_output()
            .await
            .map_err(|error| ProcessError::Capture {
                bin: self.get_bin_name(),
                error: Box::new(error),
            });

        self.post_log_command(&shared_child, instant);

        registry.remove_running(shared_child).await;

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

        match registry.cache.entry_async(self.get_cache_key()).await {
            Entry::Occupied(entry) => Ok(entry.get().clone()),
            Entry::Vacant(entry) => {
                let output = self.internal_exec_capture_output(&registry).await?;

                entry.put_entry(output.clone());

                Ok(output)
            }
        }
    }

    /// A variant of [`Self::exec_capture_output`] that streams buffered
    /// input to the child's stdin as it runs, rather than writing it all
    /// upfront, and reads stdout/stderr line by line rather than to
    /// completion. Used when [`Self::continuous_pipe`] is enabled.
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

        let stdin_handle: JoinHandle<miette::Result<()>> = task::spawn(async move {
            if let Some(mut stdin) = stdin {
                for item in items {
                    if let Err(error) = stdin.write_all(item.as_encoded_bytes()).await {
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

        let stdout_handle = spawn_capture_lines(stdout, "stdout");
        let stderr_handle = spawn_capture_lines(stderr, "stderr");

        // Attempt to create the child output
        let result = shared_child
            .wait()
            .await
            .map_err(|error| ProcessError::Capture {
                bin: self.get_bin_name(),
                error: Box::new(error),
            });

        self.post_log_command(&shared_child, instant);

        registry.remove_running(shared_child).await;

        let exit = result?;

        stdin_handle.await.into_diagnostic()??;

        let output = Output {
            exit,
            stdout: Bytes::from(stdout_handle.await.into_diagnostic()?.join("\n")),
            stderr: Bytes::from(stderr_handle.await.into_diagnostic()?.join("\n")),
        };

        self.handle_nonzero_status(&output, true)?;

        Ok(output)
    }
}

fn spawn_capture_lines<R>(reader: Option<R>, label: &'static str) -> JoinHandle<Vec<String>>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    task::spawn(async move {
        let mut logs = vec![];

        let Some(reader) = reader else {
            return logs;
        };

        let mut lines = BufReader::new(reader).lines();

        loop {
            match lines.next_line().await {
                Ok(Some(line)) => logs.push(line),
                Ok(None) => break,
                Err(error) => {
                    debug!("Failed to read {label} line: {error}");
                    break;
                }
            }
        }

        logs
    })
}
