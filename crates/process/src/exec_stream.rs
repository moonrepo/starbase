use crate::command::Command;
use crate::output::Output;
use crate::process_error::ProcessError;
use bytes::Bytes;
use starbase_console::Reporter;
use std::process::Stdio;
use std::time::Instant;

impl<R: Reporter> Command<R> {
    /// Spawn the process, inheriting stdout and stderr so its output
    /// streams directly to the terminal, and wait for it to exit. Nothing
    /// is captured; the returned [`Output`] always has empty streams.
    pub async fn exec_stream_output(&mut self) -> miette::Result<Output> {
        let set = self.get_process_set();
        let instant = Instant::now();
        let should_pass_stdin = self.should_pass_stdin();
        let mut command = self.create_async_command()?;

        command.stdin(if should_pass_stdin {
            Stdio::piped()
        } else {
            self.stdin.to_stdio()
        });

        let shared_child =
            self.spawn_child(&set, command)
                .map_err(|error| ProcessError::Stream {
                    bin: self.get_bin_name(),
                    error: Box::new(error),
                })?;

        self.pre_log_command(&shared_child);

        let (input_result, result) =
            tokio::join!(self.write_input_to_stdin(&shared_child), async {
                shared_child
                    .wait()
                    .await
                    .map_err(|error| ProcessError::Stream {
                        bin: self.get_bin_name(),
                        error: Box::new(error),
                    })
            });

        self.post_log_command(&shared_child, instant);

        // Stop tracking the child, which also kills it if it is still running
        drop(shared_child);

        input_result?;

        let exit = result?;
        let output = Output {
            exit,
            stderr: Bytes::new(),
            stdout: Bytes::new(),
        };

        self.handle_nonzero_status(&output, false)?;

        Ok(output)
    }
}
