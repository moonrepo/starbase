use crate::bin::{create_command_with_name, output_to_string, SandboxAssert};
use crate::fixture::locate_fixture;
use crate::settings::get_bin_name;
use assert_cmd::Command;
use assert_fs::prelude::*;
use assert_fs::TempDir;
use starbase_utils::fs;
use std::path::Path;
use std::process::{Command as StdCommand, Output};

pub struct Sandbox {
    pub fixture: TempDir,
}

impl Sandbox {
    pub fn path(&self) -> &Path {
        self.fixture.path()
    }

    pub fn create_file<T: AsRef<str>>(&self, name: &str, content: T) -> &Self {
        self.fixture
            .child(name)
            .write_str(content.as_ref())
            .unwrap();

        self
    }

    pub fn debug_files(&self) -> &Self {
        debug_sandbox_files(self.path());

        self
    }

    pub fn enable_git(&self) -> &Self {
        if !self.path().join(".gitignore").exists() {
            self.create_file(".gitignore", "node_modules\ntarget\n");
        }

        // Initialize a git repo so that VCS commands work
        self.run_git(|cmd| {
            cmd.args(["init", "--initial-branch", "master"]);
        });

        // We must also add the files to the index
        self.run_git(|cmd| {
            cmd.args(["add", "--all", "."]);
        });

        // And commit them... this seems like a lot of overhead?
        self.run_git(|cmd| {
            cmd.args(["commit", "-m", "Fixtures"])
                .env("GIT_AUTHOR_NAME", "Sandbox")
                .env("GIT_AUTHOR_EMAIL", "fakeemail@somedomain.dev")
                .env("GIT_COMMITTER_NAME", "Sandbox")
                .env("GIT_COMMITTER_EMAIL", "fakeemail@somedomain.dev");
        });

        self
    }

    pub fn run_git<C>(&self, handler: C) -> &Self
    where
        C: FnOnce(&mut StdCommand),
    {
        let mut cmd = StdCommand::new(if cfg!(windows) { "git.exe" } else { "git" });
        cmd.current_dir(self.path());

        handler(&mut cmd);

        let output = cmd.output().unwrap();

        if !output.status.success() {
            debug_process_output(&output);
            panic!();
        }

        self
    }

    pub fn run_bin_with_name<N, C>(&self, name: N, handler: C) -> SandboxAssert
    where
        N: AsRef<str>,
        C: FnOnce(&mut Command),
    {
        let mut cmd = create_command_with_name(self.path(), name.as_ref());

        handler(&mut cmd);

        SandboxAssert {
            inner: cmd.assert(),
            sandbox: self,
        }
    }

    pub fn run_bin<N, C>(&self, handler: C) -> SandboxAssert
    where
        C: FnOnce(&mut Command),
    {
        self.run_bin_with_name(get_bin_name(), handler)
    }
}

pub fn create_temp_dir() -> TempDir {
    TempDir::new().unwrap()
}

pub fn create_sandbox<N: AsRef<str>>(fixture: N) -> Sandbox {
    let temp_dir = create_temp_dir();

    temp_dir
        .copy_from(locate_fixture(fixture), &["**/*"])
        .unwrap();

    Sandbox { fixture: temp_dir }
}

pub fn debug_sandbox_files<P: AsRef<Path>>(dir: P) {
    println!("SANDBOX:");

    let dir = dir.as_ref();

    for entry in fs::read_dir_all(dir).unwrap() {
        println!("- {}", entry.path().to_string_lossy());
    }
}

pub fn debug_process_output(output: &Output) {
    println!("STDOUT:\n{}\n", output_to_string(&output.stdout));
    println!("STDERR:\n{}\n", output_to_string(&output.stderr));
    println!("STATUS:\n{:#?}", output.status);
}
