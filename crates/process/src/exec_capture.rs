use crate::command::Command;
use crate::exec::with_timeout;
use crate::output::Output;
use crate::output_cache::OutputCache;
use crate::process_error::ProcessError;
use crate::process_set::ProcessSet;
use bytes::Bytes;
use miette::IntoDiagnostic;
use starbase_console::Reporter;
use std::io;
use std::process::Stdio;
use std::time::Instant;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::task::{self, JoinHandle};
use tracing::debug;

impl<R: Reporter> Command<R> {
    async fn internal_exec_capture_output(&mut self, set: &ProcessSet) -> miette::Result<Output> {
        let instant = Instant::now();
        let should_pass_stdin = self.should_pass_stdin();
        let mut command = self.create_async_command()?;

        command
            .stdin(if should_pass_stdin {
                Stdio::piped()
            } else {
                self.stdin.to_stdio()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let shared_child = self
            .spawn_child(set, command, self.should_contain_tree())
            .map_err(|error| ProcessError::Capture {
                bin: self.get_bin_name(),
                error: Box::new(error),
            })?;

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

        // Stop tracking the child, which also kills it if it is still running
        drop(shared_child);

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

        let set = self.get_process_set();

        if !self.should_cache_output() {
            let result = with_timeout(self.timeout, self.internal_exec_capture_output(&set)).await;

            return self.check_timeout(result);
        }

        let key = self.get_output_cache_key("capture");
        let cache = OutputCache::instance();

        if let Some(output) = cache.get(&key).await {
            return self.handle_cached_output(output);
        }

        let result = with_timeout(self.timeout, self.internal_exec_capture_output(&set)).await;
        let output = self.check_timeout(result)?;

        // A run ended by a signal, such as a shutdown, may be cut short
        if output.exit.signal().is_none() {
            cache.put(key, output.clone()).await;
        }

        Ok(output)
    }

    /// A variant of [`Self::exec_capture_output`] that streams buffered
    /// input to the child's stdin as it runs, rather than writing it all
    /// upfront, while capturing stdout and stderr as raw bytes. Used when
    /// [`Self::continuous_pipe`] is enabled. Force-killing stops capture
    /// readers and may truncate unread output.
    pub async fn exec_capture_continuous_output(&mut self) -> miette::Result<Output> {
        let result =
            with_timeout(self.timeout, self.internal_exec_capture_continuous_output()).await;

        self.check_timeout(result)
    }

    async fn internal_exec_capture_continuous_output(&mut self) -> miette::Result<Output> {
        let set = self.get_process_set();
        let instant = Instant::now();
        let mut command = self.create_async_command()?;

        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let shared_child = self
            .spawn_child(&set, command, self.should_contain_tree())
            .map_err(|error| ProcessError::Capture {
                bin: self.get_bin_name(),
                error: Box::new(error),
            })?;
        let stdin = shared_child.take_stdin().await;
        let stdout = shared_child.take_stdout().await;
        let stderr = shared_child.take_stderr().await;

        self.pre_log_command(&shared_child);

        let items = self.input.clone();
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
                pid: shared_child.id(),
                stdout: Bytes::from(stdout_handle.await.into_diagnostic()?),
                stderr: Bytes::from(stderr_handle.await.into_diagnostic()?),
            })
        }
        .await;

        // Stop tracking the child, which also kills it if it is still running
        drop(shared_child);

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
