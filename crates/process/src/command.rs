use crate::arg::*;
use crate::env::*;
use crate::exe::*;
use crate::helpers::get_default_shell;
use rustc_hash::{FxHashMap, FxHasher};
use starbase_console::Console;
use starbase_console::Reporter;
use starbase_shell::{ShellType, join_exe_args};
use starbase_styles::color;
use std::collections::VecDeque;
use std::env;
use std::ffi::{OsStr, OsString};
use std::hash::Hasher;
use std::sync::Arc;

/// Debugging and environment-detection flags for a [`Command`], typically
/// set by the host application rather than by end users.
#[derive(Debug, Default)]
pub struct CommandDebug {
    /// Only log environment variables whose key starts with one of these
    /// prefixes, unless [`Self::print_env`] is enabled.
    pub env_key_prefixes: Vec<String>,

    /// Whether this command is running inside a long-lived daemon
    /// process. Disables output caching, as a daemon's cache would
    /// outlive the state it was computed from.
    pub is_daemon_env: bool,

    /// Whether this command is running inside a test. Disables output
    /// caching and color inheritance, to keep snapshots deterministic.
    pub is_test_env: bool,

    /// Log every environment variable, ignoring
    /// [`Self::env_key_prefixes`].
    pub print_env: bool,

    /// Log the full stdin input, rather than truncating a large payload.
    pub print_input: bool,

    /// The environment variable holding the workspace root, used to
    /// display the working directory relative to it when logging.
    pub root_dir_env_key: Option<String>,
}

/// A builder for a child process, wrapping the standard library's
/// `Command` with shell wrapping, environment variable behaviors,
/// buffered input, output caching, and console integration.
#[derive(Debug)]
pub struct Command<R: Reporter> {
    /// Arguments to pass to the executable.
    pub args: VecDeque<Arg>,

    /// Reuse a prior identical run's output instead of spawning again.
    /// See [`Self::get_cache_key`] for what counts as identical.
    pub cache: bool,

    /// Continuously write to stdin and read from stdout
    pub continuous_pipe: bool,

    /// The working directory to spawn the process in.
    pub cwd: Option<OsString>,

    /// Debugging and environment-detection flags.
    pub debug: CommandDebug,

    /// Environment variables to apply to the child process.
    pub env: FxHashMap<OsString, Env>,

    /// The binary or script to execute.
    pub exe: Executable,

    /// Convert non-zero exits to errors
    pub error_on_nonzero: bool,

    /// Values to pass to stdin
    pub input: Vec<OsString>,

    /// Paths to prepend to `PATH`
    pub paths: VecDeque<OsString>,

    /// Prefix to prepend to all log lines
    pub prefix: Option<String>,

    /// Log the command to the terminal before running
    pub print_command: bool,

    /// Shell to wrap executing commands in
    pub shell: Option<ShellType>,

    /// Console to write output to
    pub console: Option<Arc<Console<R>>>,
}

impl<R: Reporter> Command<R> {
    /// Create a command that runs `bin` with the default shell.
    pub fn new<T: AsRef<OsStr>>(bin: T) -> Self {
        Command {
            args: VecDeque::new(),
            cache: false,
            continuous_pipe: false,
            cwd: None,
            debug: CommandDebug::default(),
            env: FxHashMap::default(),
            exe: Executable::Binary(Arg {
                quoted_value: None,
                value: bin.as_ref().to_os_string(),
            }),
            error_on_nonzero: true,
            input: vec![],
            paths: VecDeque::new(),
            prefix: None,
            print_command: false,
            shell: Some(get_default_shell()),
            console: None,
        }
    }

    /// Create a command that runs `bin`, accepting a pre-built [`Arg`]
    /// (e.g. one with a quoted value) rather than a plain string.
    pub fn new_bin<T: Into<Arg>>(bin: T) -> Self {
        let mut command = Self::new("");
        command.exe = Executable::Binary(bin.into());
        command
    }

    /// Create a command that runs a full shell script, e.g.
    /// `"git commit --allow-empty"`, rather than a single binary.
    pub fn new_script<T: AsRef<OsStr>>(script: T) -> Self {
        let mut command = Self::new("");
        command.exe = Executable::Script(script.as_ref().to_os_string());
        command
    }

    /// Append a single argument.
    pub fn arg<A: Into<Arg>>(&mut self, arg: A) -> &mut Self {
        self.args.push_back(arg.into());
        self
    }

    /// Append a single argument, unless an identical one is already
    /// present (see [`Self::contains_arg`]).
    pub fn arg_if_missing<A: Into<Arg>>(&mut self, arg: A) -> &mut Self {
        let arg = arg.into();

        if !self.contains_arg(&arg.value) {
            self.arg(arg);
        }

        self
    }

    /// Append many arguments.
    pub fn args<I, A>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = A>,
        A: Into<Arg>,
    {
        for arg in args {
            self.arg(arg);
        }

        self
    }

    /// Return true if an argument with this raw or quoted value has
    /// already been added.
    pub fn contains_arg<A>(&self, arg: A) -> bool
    where
        A: AsRef<OsStr>,
    {
        let arg = arg.as_ref();
        self.args
            .iter()
            .any(|a| a.value == arg || a.quoted_value.as_ref().is_some_and(|aa| aa == arg))
    }

    /// Return true if this key has an explicit [`Env`] behavior set,
    /// including [`Env::Unset`].
    pub fn contains_env<K>(&self, key: K) -> bool
    where
        K: AsRef<OsStr>,
    {
        self.env.contains_key(key.as_ref())
    }

    /// Set the working directory to spawn the process in.
    pub fn cwd<P: AsRef<OsStr>>(&mut self, dir: P) -> &mut Self {
        self.cwd = Some(dir.as_ref().to_os_string());
        self
    }

    /// Always set and overwrite this environment variable.
    pub fn env<K, V>(&mut self, key: K, value: V) -> &mut Self
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.env_opt(key, Some(value))
    }

    /// Set this environment variable, or unset it if `value` is `None`.
    pub fn env_opt<K, V>(&mut self, key: K, value: Option<V>) -> &mut Self
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.env_with_behavior(
            key,
            match value {
                Some(v) => Env::Set(v.as_ref().to_os_string()),
                None => Env::Unset,
            },
        )
    }

    /// Unset this environment variable and don't inherit it from the
    /// parent process.
    pub fn env_remove<K>(&mut self, key: K) -> &mut Self
    where
        K: AsRef<OsStr>,
    {
        self.env_with_behavior(key, Env::Unset)
    }

    /// Set this environment variable with an explicit [`Env`] behavior.
    pub fn env_with_behavior<K>(&mut self, key: K, value: Env) -> &mut Self
    where
        K: AsRef<OsStr>,
    {
        self.env.insert(key.as_ref().to_os_string(), value);
        self
    }

    /// Always set and overwrite many environment variables. See
    /// [`Self::env`].
    pub fn envs<I, K, V>(&mut self, vars: I) -> &mut Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        for (k, v) in vars {
            self.env(k, v);
        }

        self
    }

    /// Set or unset many environment variables. See [`Self::env_opt`].
    pub fn envs_opt<I, K, V>(&mut self, vars: I) -> &mut Self
    where
        I: IntoIterator<Item = (K, Option<V>)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        for (k, v) in vars {
            self.env_opt(k, v);
        }

        self
    }

    /// Unset many environment variables. See [`Self::env_remove`].
    pub fn envs_remove<I, V>(&mut self, vars: I) -> &mut Self
    where
        I: IntoIterator<Item = V>,
        V: AsRef<OsStr>,
    {
        for v in vars {
            self.env_remove(v);
        }

        self
    }

    /// Inherit `NO_COLOR`/`FORCE_COLOR` from the current environment, and
    /// pin a consistent terminal size, unless a test environment or an
    /// explicit value on this command overrides it.
    pub fn inherit_colors(&mut self) -> &mut Self {
        // Don't show colors in our own tests, as it disrupts snapshots,
        // and only inherit colors if the current command hasn't
        // explicitly configured these variables
        if !self.debug.is_test_env
            && !self.contains_env("NO_COLOR")
            && !self.contains_env("FORCE_COLOR")
            && env::var_os("NO_COLOR").is_none()
            && env::var_os("FORCE_COLOR").is_none()
        {
            let level = color::supports_color().to_string();

            self.env_remove("NO_COLOR");
            self.env("FORCE_COLOR", &level);
            self.env("CLICOLOR_FORCE", &level);
        }

        // Force a terminal width so that we have consistent sizing
        // in our cached output, and its the same across all machines
        // https://help.gnome.org/users/gnome-terminal/stable/app-terminal-sizes.html.en
        self.env("COLUMNS", "80");
        self.env("LINES", "24");

        self
    }

    /// Append values to write to the child's stdin, joined with spaces
    /// when written.
    pub fn input<I, V>(&mut self, input: I) -> &mut Self
    where
        I: IntoIterator<Item = V>,
        V: AsRef<OsStr>,
    {
        for i in input {
            self.input.push(i.as_ref().to_os_string());
        }

        self
    }

    /// Append paths to the end of the child's `PATH`.
    pub fn append_paths<I, V>(&mut self, list: I) -> &mut Self
    where
        I: IntoIterator<Item = V>,
        V: AsRef<OsStr>,
    {
        for path in list {
            self.paths.push_back(path.as_ref().to_os_string());
        }

        self
    }

    /// Prepend paths to the front of the child's `PATH`, in the order
    /// given.
    pub fn prepend_paths<I, V>(&mut self, list: I) -> &mut Self
    where
        I: IntoIterator<Item = V>,
        V: AsRef<OsStr>,
    {
        let mut paths = vec![];

        for path in list {
            paths.push(path.as_ref().to_os_string());
        }

        for path in paths.into_iter().rev() {
            self.paths.push_front(path);
        }

        self
    }

    /// Return the raw (unquoted) arguments as strings.
    pub fn get_args_list(&self) -> Vec<String> {
        self.args
            .iter()
            .map(|arg| arg.value.to_string_lossy().to_string())
            .collect()
    }

    /// Return the binary name, or the first word of the script if this
    /// command runs a script.
    pub fn get_bin_name(&self) -> String {
        match &self.exe {
            Executable::Binary(bin) => bin.value.to_string_lossy().to_string(),
            Executable::Script(script) => {
                let script = script.to_string_lossy();

                match script.find(' ') {
                    Some(index) => script[0..index].to_string(),
                    None => script.into_owned(),
                }
            }
        }
    }

    /// Compute a stable hash identifying this command's executable,
    /// arguments, environment, working directory, and input. Two commands
    /// with the same key are expected to produce the same output.
    pub fn get_cache_key(&self) -> String {
        let mut hasher = FxHasher::default();

        // Length-prefix each field, otherwise consecutive values hash
        // ambiguously, like ("ab", "c") and ("a", "bc")
        fn write(hasher: &mut FxHasher, value: &OsStr) {
            let bytes = value.as_encoded_bytes();
            hasher.write_usize(bytes.len());
            hasher.write(bytes);
        }

        // Sort env vars, as map iteration order is not guaranteed,
        // and the key must be stable for identical commands
        let mut env = self.env.iter().collect::<Vec<_>>();
        env.sort_by(|a, b| a.0.cmp(b.0));

        for (key, value) in env {
            write(&mut hasher, key);

            match value {
                Env::Set(value) => {
                    hasher.write_u8(1);
                    write(&mut hasher, value);
                }
                Env::SetIfMissing(value) => {
                    hasher.write_u8(2);
                    write(&mut hasher, value);
                }
                Env::Unset => {
                    hasher.write_u8(0);
                }
            };
        }

        match &self.exe {
            Executable::Binary(exe) => {
                write(&mut hasher, &exe.value);
            }
            Executable::Script(exe) => {
                write(&mut hasher, exe);
            }
        };

        for arg in &self.args {
            write(&mut hasher, &arg.value);
        }

        if let Some(cwd) = &self.cwd {
            write(&mut hasher, cwd);
        }

        for arg in &self.input {
            write(&mut hasher, arg);
        }

        hasher.finish().to_string()
    }

    /// Render this command as a single line for display, e.g. in logs.
    /// This is for humans only and is never executed: shell wrapping is
    /// marked with curly quotes to distinguish it from real quoting
    /// within the command, and large input is truncated unless
    /// [`CommandDebug::print_input`] is set.
    pub fn get_command_line(&self, with_shell: bool, with_input: bool) -> String {
        let shell = self.shell.unwrap_or_default().build();
        let use_shell = with_shell && (self.shell.is_some() || self.exe.requires_shell());
        let mut line = OsString::new();

        // Curly quotes are intentional, so that the shell wrapper is
        // distinguishable from real quoting within the command itself.
        // This output is only used for logs, never executed!
        if use_shell {
            line.push(shell.to_string());
            line.push(" -c “");
        }

        match &self.exe {
            Executable::Binary(bin) => {
                line.push(join_exe_args(&shell, bin, &self.args, false));
            }
            Executable::Script(script) => {
                line.push(script);
            }
        };

        if use_shell {
            line.push("”");
        }

        if with_input && !self.input.is_empty() {
            let input = self.input.join(OsStr::new(" "));

            line.push(" - ");

            if input.len() > 200 && !self.debug.print_input {
                line.push(format!(
                    "(truncated input, {} total bytes)",
                    self.get_input_size()
                ));
            } else {
                line.push(input);
            }
        }

        line.to_string_lossy().trim().replace('\n', " ")
    }

    /// Return the total byte length of the buffered stdin input.
    pub fn get_input_size(&self) -> usize {
        self.input.iter().map(|i| i.len()).sum()
    }

    /// Return the prefix to prepend to log lines, if set.
    pub fn get_prefix(&self) -> Option<&str> {
        self.prefix.as_deref()
    }

    /// Return the binary name, or the full script if this command runs
    /// a script.
    pub fn get_script(&self) -> String {
        match &self.exe {
            Executable::Binary(bin) => bin.value.to_string_lossy().to_string(),
            Executable::Script(script) => script.to_string_lossy().to_string(),
        }
    }

    /// Run the executable directly, without wrapping it in a shell.
    /// A command that runs a script still requires a shell, so this has
    /// no effect on it.
    pub fn no_shell(&mut self) -> &mut Self {
        self.shell = None;
        self
    }

    /// Replace the executable with a binary.
    pub fn set_bin<T: Into<Arg>>(&mut self, bin: T) -> &mut Self {
        self.exe = Executable::Binary(bin.into());
        self
    }

    /// Set whether to reuse a prior identical run's output. See
    /// [`Self::cache`].
    pub fn set_cache(&mut self, state: bool) -> &mut Self {
        self.cache = state;
        self
    }

    /// Attach a console to write output to.
    pub fn set_console(&mut self, console: Arc<Console<R>>) -> &mut Self {
        self.console = Some(console);
        self
    }

    /// Set whether to continuously write to stdin and read from stdout,
    /// rather than writing all input upfront and reading output only
    /// once the process exits. See [`Self::continuous_pipe`].
    pub fn set_continuous_pipe(&mut self, state: bool) -> &mut Self {
        self.continuous_pipe = state;
        self
    }

    /// Set whether a non-zero exit should be converted into an error.
    /// See [`Self::error_on_nonzero`].
    pub fn set_error_on_nonzero(&mut self, state: bool) -> &mut Self {
        self.error_on_nonzero = state;
        self
    }

    /// Set the prefix to prepend to all log lines.
    pub fn set_prefix(&mut self, prefix: &str) -> &mut Self {
        self.prefix = Some(prefix.to_owned());
        self
    }

    /// Set whether to log the command line to the console before
    /// running it. See [`Self::print_command`].
    pub fn set_print_command(&mut self, state: bool) -> &mut Self {
        self.print_command = state;
        self
    }

    /// Replace the executable with a full shell script. Enables the
    /// default shell if none is set, as scripts always require one.
    pub fn set_script<T: AsRef<OsStr>>(&mut self, script: T) -> &mut Self {
        if self.shell.is_none() {
            self.shell = Some(get_default_shell());
        }

        self.exe = Executable::Script(script.as_ref().to_os_string());
        self
    }

    /// Set the shell to wrap the executable in.
    pub fn set_shell(&mut self, shell: ShellType) -> &mut Self {
        self.shell = Some(shell);
        self
    }

    /// Return true if output caching is enabled and not suppressed by a
    /// test or daemon environment.
    pub fn should_cache_output(&self) -> bool {
        self.cache && !self.debug.is_test_env && !self.debug.is_daemon_env
    }

    /// Return true if a non-zero exit should be converted into an error.
    pub fn should_error_nonzero(&self) -> bool {
        self.error_on_nonzero
    }

    /// Return true if there's buffered input to write to stdin.
    pub fn should_pass_stdin(&self) -> bool {
        !self.input.is_empty()
    }

    /// Convert this command to use a different console reporter type,
    /// carrying over every other field and attaching the given console.
    pub fn with_console<T: Reporter>(self, console: Arc<Console<T>>) -> Command<T> {
        Command {
            args: self.args,
            cache: self.cache,
            continuous_pipe: self.continuous_pipe,
            cwd: self.cwd,
            debug: self.debug,
            env: self.env,
            exe: self.exe,
            error_on_nonzero: self.error_on_nonzero,
            input: self.input,
            paths: self.paths,
            prefix: self.prefix,
            print_command: self.print_command,
            shell: self.shell,
            console: Some(console),
        }
    }
}
