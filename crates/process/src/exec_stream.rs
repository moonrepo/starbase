use crate::command::Command;
use crate::output::Output;
use crate::process_error::ProcessError;
use crate::process_registry::ProcessRegistry;
use bytes::Bytes;
use starbase_console::Reporter;
use std::process::Stdio;
use std::time::Instant;

impl<R: Reporter> Command<R> {
    pub async fn exec_stream_output(&mut self) -> miette::Result<Output> {
        let registry = ProcessRegistry::instance();
        let instant = Instant::now();
        let mut command = self.create_async_command()?;

        let child = if self.should_pass_stdin() {
            command.stdin(Stdio::piped());

            let mut child = command.spawn().map_err(|error| ProcessError::Stream {
                bin: self.get_bin_name(),
                error: Box::new(error),
            })?;

            self.write_input_to_child(&mut child).await?;

            child
        } else {
            command.spawn().map_err(|error| ProcessError::Stream {
                bin: self.get_bin_name(),
                error: Box::new(error),
            })?
        };

        let shared_child = registry.add_running(child).await;

        self.pre_log_command(&shared_child);

        let result = shared_child
            .wait()
            .await
            .map_err(|error| ProcessError::Stream {
                bin: self.get_bin_name(),
                error: Box::new(error),
            });

        self.post_log_command(&shared_child, instant);

        registry.remove_running(shared_child).await;

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
