use crate::command::Command;
use crate::env::Env;
use crate::exe::Executable;
use crate::helpers::format_command_line;
use crate::output::Output;
use crate::process_error::ProcessError;
use crate::shared_child::SharedChild;
use miette::IntoDiagnostic;
use rustc_hash::FxHashMap;
use starbase_console::Reporter;
use starbase_shell::join_exe_args;
use starbase_styles::color;
use std::env;
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::PathBuf;
use std::process::Command as StdCommand;
use std::time::Instant;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command as TokioCommand};
use tracing::{debug, enabled};

impl<R: Reporter> Command<R> {
    pub fn create_sync_command(&self) -> miette::Result<StdCommand> {
        // When the command is wrapped in a shell, we need to create a single
        // string of the full command line with args quoted correctly, as
        // it's passed as a single argument to the shell: `bash -c "command line"`
        let mut command = if self.shell.is_some() || self.exe.requires_shell() {
            let shell_type = self.shell.unwrap_or_default();
            let shell = shell_type.build();

            let script = match &self.exe {
                Executable::Binary(bin) => join_exe_args(&shell, bin, &self.args, false),
                Executable::Script(script) => script.to_owned(),
            };

            let mut command = shell.create_wrapped_command_with(script);

            // Non-interactive bash sources the file referenced by `BASH_ENV`
            // on startup, after the process environment has been applied.
            // CI providers like CircleCI persist environment variables between
            // steps through this file, so an `export PATH=...` within it will
            // overwrite the `PATH` we explicitly set for this process.
            // https://github.com/moonrepo/moon/issues/1998
            if matches!(shell_type, starbase_shell::ShellType::Bash)
                && !self.env.contains_key(OsStr::new("BASH_ENV"))
            {
                command.env_remove("BASH_ENV");
            }

            command
        }
        // When the command is not in a shell, we can create a standard command
        // and pass the non-quoted args separately
        else {
            let mut command = StdCommand::new(self.exe.as_os_str());

            for arg in &self.args {
                command.arg(&arg.value);
            }

            command
        };

        // Then set explicit vars
        for (key, value) in &self.env {
            match value {
                Env::Set(value) => {
                    command.env(key, value);
                }
                Env::SetIfMissing(value) => {
                    if env::var_os(key).is_none() {
                        command.env(key, value);
                    }
                }
                Env::Unset => {
                    command.env_remove(key);
                }
            };
        }

        if let Some(cwd) = &self.cwd {
            command.current_dir(cwd);
            command.env("PWD", cwd);
        }

        // And lastly inherit lookup paths
        let path_key = OsString::from("PATH");

        if !self.env.contains_key(&path_key) && !self.paths.is_empty() {
            let mut paths = self.paths.iter().map(PathBuf::from).collect::<Vec<_>>();

            if let Some(path_value) = env::var_os(&path_key) {
                paths.extend(env::split_paths(&path_value));
            }

            command.env(path_key, env::join_paths(paths).into_diagnostic()?);
        }

        Ok(command)
    }

    pub fn create_async_command(&self) -> miette::Result<TokioCommand> {
        Ok(TokioCommand::from(self.create_sync_command()?))
    }

    pub(crate) fn handle_nonzero_status(
        &mut self,
        output: &Output,
        with_message: bool,
    ) -> miette::Result<()> {
        if self.should_error_nonzero() && !output.success() {
            return Err(output.to_error(self.get_bin_name(), with_message).into());
        }

        Ok(())
    }

    pub(crate) fn pre_log_command(&self, child: &SharedChild) {
        let root_dir = match &self.debug.root_dir_env_key {
            Some(base_key) => {
                let key = OsString::from(base_key);

                match self.env.get(&key).and_then(|var| var.get_value()) {
                    Some(value) => value.to_os_string(),
                    None => env::var_os(&key).unwrap_or_default(),
                }
            }
            None => OsString::new(),
        };

        // Determine root dir and working dir
        let root_dir = if root_dir.is_empty() {
            env::current_dir().unwrap_or_default()
        } else {
            PathBuf::from(root_dir)
        };

        let working_dir = PathBuf::from(self.cwd.as_deref().unwrap_or(root_dir.as_os_str()));

        // Print the command line to the console
        if let Some(console) = self.console.as_ref()
            && self.print_command
            && !console.out.is_quiet()
        {
            let command_line = self.get_command_line(false, false);

            let _ =
                console
                    .out
                    .write_line(format_command_line(&command_line, &root_dir, &working_dir));
        }

        // Avoid all this overhead if we're not logging
        if !enabled!(tracing::Level::DEBUG) {
            return;
        }

        let env_vars: FxHashMap<&OsString, &OsString> = self
            .env
            .iter()
            .filter_map(|(key, value)| {
                if value == &Env::Unset {
                    None
                } else if self.debug.print_env
                    || key.to_str().is_some_and(|k| {
                        self.debug
                            .env_key_prefixes
                            .iter()
                            .any(|prefix| k.starts_with(prefix))
                    })
                {
                    Some((key, value.get_value().unwrap()))
                } else {
                    None
                }
            })
            .collect();

        let shell = self.shell.as_ref().map(|sh| sh.to_string());
        let input_size = (!self.input.is_empty()).then(|| self.get_input_size());
        let command_line = self.get_command_line(true, true);

        debug!(
            pid = child.id(),
            shell,
            env = ?env_vars,
            cwd = ?working_dir,
            input_size,
            "Running command {}",
            color::shell(command_line)
        );
    }

    pub(crate) fn post_log_command(&self, child: &SharedChild, instant: Instant) {
        debug!(pid = child.id(), "Ran command in {:?}", instant.elapsed());
    }

    pub(crate) async fn write_input_to_child(&self, child: &mut Child) -> miette::Result<()> {
        let mut stdin = child.stdin.take().expect("Unable to write stdin!");

        if let Err(error) = stdin
            .write_all(self.input.join(OsStr::new(" ")).as_encoded_bytes())
            .await
        {
            // The child exited, or closed its stdin, before consuming all
            // input. Not a failure in itself: the child's exit status is
            // the outcome.
            if error.kind() != io::ErrorKind::BrokenPipe {
                return Err(ProcessError::WriteInput {
                    bin: self.get_bin_name(),
                    error: Box::new(error),
                }
                .into());
            }

            debug!(
                bin = self.get_bin_name(),
                "Child process closed stdin before all input was written"
            );
        }

        drop(stdin);

        Ok(())
    }
}
