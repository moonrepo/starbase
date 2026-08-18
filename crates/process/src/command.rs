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

#[derive(Debug, Default)]
pub struct CommandDebug {
    pub is_daemon_env: bool,
    pub is_test_env: bool,
    pub print_input: bool,
}

#[derive(Debug)]
pub struct Command<R: Reporter> {
    pub args: VecDeque<Arg>,

    pub cache: bool,

    /// Continuously write to stdin and read from stdout
    pub continuous_pipe: bool,

    pub cwd: Option<OsString>,

    pub debug: CommandDebug,

    pub env: FxHashMap<OsString, Env>,

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

    pub fn new_bin<T: Into<Arg>>(bin: T) -> Self {
        let mut command = Self::new("");
        command.exe = Executable::Binary(bin.into());
        command
    }

    pub fn new_script<T: AsRef<OsStr>>(script: T) -> Self {
        let mut command = Self::new("");
        command.exe = Executable::Script(script.as_ref().to_os_string());
        command
    }

    pub fn arg<A: Into<Arg>>(&mut self, arg: A) -> &mut Self {
        self.args.push_back(arg.into());
        self
    }

    pub fn arg_if_missing<A: Into<Arg>>(&mut self, arg: A) -> &mut Self {
        let arg = arg.into();

        if !self.contains_arg(&arg.value) {
            self.arg(arg);
        }

        self
    }

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

    pub fn contains_arg<A>(&self, arg: A) -> bool
    where
        A: AsRef<OsStr>,
    {
        let arg = arg.as_ref();
        self.args
            .iter()
            .any(|a| a.value == arg || a.quoted_value.as_ref().is_some_and(|aa| aa == arg))
    }

    pub fn contains_env<K>(&self, key: K) -> bool
    where
        K: AsRef<OsStr>,
    {
        self.env.contains_key(key.as_ref())
    }

    pub fn cwd<P: AsRef<OsStr>>(&mut self, dir: P) -> &mut Self {
        self.cwd = Some(dir.as_ref().to_os_string());
        self
    }

    pub fn env<K, V>(&mut self, key: K, value: V) -> &mut Self
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.env_opt(key, Some(value))
    }

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

    pub fn env_remove<K>(&mut self, key: K) -> &mut Self
    where
        K: AsRef<OsStr>,
    {
        self.env_with_behavior(key, Env::Unset)
    }

    pub fn env_with_behavior<K>(&mut self, key: K, value: Env) -> &mut Self
    where
        K: AsRef<OsStr>,
    {
        self.env.insert(key.as_ref().to_os_string(), value);
        self
    }

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

    pub fn get_args_list(&self) -> Vec<String> {
        self.args
            .iter()
            .map(|arg| arg.value.to_string_lossy().to_string())
            .collect()
    }

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

    pub fn get_input_size(&self) -> usize {
        self.input.iter().map(|i| i.len()).sum()
    }

    pub fn get_prefix(&self) -> Option<&str> {
        self.prefix.as_deref()
    }

    pub fn get_script(&self) -> String {
        match &self.exe {
            Executable::Binary(bin) => bin.value.to_string_lossy().to_string(),
            Executable::Script(script) => script.to_string_lossy().to_string(),
        }
    }

    pub fn no_shell(&mut self) -> &mut Self {
        self.shell = None;
        self
    }

    pub fn set_bin<T: Into<Arg>>(&mut self, bin: T) -> &mut Self {
        self.exe = Executable::Binary(bin.into());
        self
    }

    pub fn set_cache(&mut self, state: bool) -> &mut Self {
        self.cache = state;
        self
    }

    pub fn set_console(&mut self, console: Arc<Console<R>>) -> &mut Self {
        self.console = Some(console);
        self
    }

    pub fn set_continuous_pipe(&mut self, state: bool) -> &mut Self {
        self.continuous_pipe = state;
        self
    }

    pub fn set_error_on_nonzero(&mut self, state: bool) -> &mut Self {
        self.error_on_nonzero = state;
        self
    }

    pub fn set_prefix(&mut self, prefix: &str) -> &mut Self {
        self.prefix = Some(prefix.to_owned());
        self
    }

    pub fn set_print_command(&mut self, state: bool) -> &mut Self {
        self.print_command = state;
        self
    }

    pub fn set_script<T: AsRef<OsStr>>(&mut self, script: T) -> &mut Self {
        if self.shell.is_none() {
            self.shell = Some(get_default_shell());
        }

        self.exe = Executable::Script(script.as_ref().to_os_string());
        self
    }

    pub fn set_shell(&mut self, shell: ShellType) -> &mut Self {
        self.shell = Some(shell);
        self
    }

    pub fn should_cache_output(&self) -> bool {
        self.cache && !self.debug.is_test_env && !self.debug.is_daemon_env
    }

    pub fn should_error_nonzero(&self) -> bool {
        self.error_on_nonzero
    }

    pub fn should_pass_stdin(&self) -> bool {
        !self.input.is_empty()
    }

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
