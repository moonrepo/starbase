use crate::sandbox::{Sandbox, debug_process_output, debug_sandbox_files};
use crate::settings::SandboxSettings;
use assert_cmd::assert::Assert;
use starbase_utils::dirs::home_dir;
use std::path::Path;

/// Create a command to run with the provided binary name.
pub fn create_command_with_name<P: AsRef<Path>, N: AsRef<str>>(
    path: P,
    name: N,
    settings: &SandboxSettings,
) -> assert_cmd::Command {
    #[allow(deprecated)]
    let mut cmd = assert_cmd::Command::cargo_bin(name.as_ref()).unwrap();
    cmd.current_dir(path);
    cmd.timeout(std::time::Duration::from_secs(settings.timeout));
    cmd.env("RUST_BACKTRACE", "1");
    cmd.env("STARBASE_LOG", "trace");
    cmd.env("STARBASE_TEST", "true");
    cmd.envs(&settings.env);
    cmd
}

/// Create a command to run. Will default the binary name to the `BIN_NAME` setting,
/// or the `CARGO_BIN_NAME` environment variable.
pub fn create_command<P: AsRef<Path>>(path: P, settings: &SandboxSettings) -> assert_cmd::Command {
    create_command_with_name(path, &settings.bin, settings)
}

/// Convert a binary output to a string.
pub fn output_to_string(data: &[u8]) -> String {
    String::from_utf8(data.to_vec()).unwrap_or_default()
}

/// Convert the stdout and stderr output to a string.
pub fn get_assert_output(assert: &Assert) -> String {
    get_assert_stdout_output(assert) + &get_assert_stderr_output(assert)
}

/// Convert the stdout output to a string.
pub fn get_assert_stdout_output(assert: &Assert) -> String {
    output_to_string(&assert.get_output().stdout)
}

/// Convert the stderr output to a string.
pub fn get_assert_stderr_output(assert: &Assert) -> String {
    output_to_string(&assert.get_output().stderr)
}

pub struct SandboxAssert<'s> {
    pub inner: Assert,
    pub sandbox: &'s Sandbox,
}

impl SandboxAssert<'_> {
    /// Debug all files in the sandbox and the command's output.
    pub fn debug(&self) -> &Self {
        debug_sandbox_files(self.sandbox.path());
        println!("\n");
        debug_process_output(self.inner.get_output());

        self
    }

    /// Ensure the command returned the expected code.
    pub fn code(self, num: i32) -> Assert {
        self.inner.code(num)
    }

    /// Ensure the command failed.
    pub fn failure(self) -> Assert {
        self.inner.failure()
    }

    /// Ensure the command succeeded.
    pub fn success(self) -> Assert {
        self.inner.success()
    }

    /// Return stderr as a string.
    pub fn stderr(&self) -> String {
        get_assert_stderr_output(&self.inner)
    }

    /// Return stdout as a string.
    pub fn stdout(&self) -> String {
        get_assert_stdout_output(&self.inner)
    }

    /// Return a combined output of stdout and stderr.
    /// Will replace the sandbox root and home directories.
    pub fn output(&self) -> String {
        let mut output = String::new();
        output.push_str(&self.sandbox.settings.apply_log_filters(self.stderr()));
        output.push_str(&self.sandbox.settings.apply_log_filters(self.stdout()));

        // Replace fixture path
        let root = self.sandbox.path().to_str().unwrap();

        output = output.replace(root, "<WORKSPACE>");
        output = output.replace(&root.replace('\\', "/"), "<WORKSPACE>");

        // Replace home dir
        if let Some(home_dir) = home_dir() {
            let home = home_dir.to_str().unwrap();

            output = output.replace(home, "~");
            output = output.replace(&home.replace('\\', "/"), "~");
        }

        // Replace private path weirdness
        output.replace("/private<", "<")
    }

    /// Like `output()` but also replaces backslashes with forward slashes.
    /// Useful for standardizing snapshots across platforms.
    pub fn output_standardized(&self) -> String {
        self.output().replace('\\', "/")
    }
}
