//! Prepare commands and freeze the inputs used by output cache keys.

use super::cache::CacheKey;
use crate::Command;
use miette::IntoDiagnostic;
use starbase_console::Reporter;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::process::Stdio;

pub(super) struct Prepared {
    pub(super) command: tokio::process::Command,
    pub(super) input: Vec<u8>,
    pub(super) key: CacheKey,
}

impl Prepared {
    pub(super) fn new<R: Reporter>(command: &Command<R>) -> miette::Result<Self> {
        let mut native = command.create_sync_command()?;
        let cwd = std::env::current_dir().into_diagnostic()?;
        let cwd = native
            .get_current_dir()
            .map_or(cwd.clone(), |path| cwd.join(path));
        let mut env = std::env::vars_os().collect::<BTreeMap<_, _>>();
        for (key, value) in native.get_envs() {
            #[cfg(windows)]
            env.retain(|existing, _| {
                !existing
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&key.to_string_lossy())
            });
            match value {
                Some(value) => {
                    env.insert(key.to_owned(), value.to_owned());
                }
                None => {
                    env.remove(key);
                }
            }
        }
        // Freeze inherited values used in the key and by the actual child.
        native.env_clear().envs(&env).current_dir(&cwd);
        let input = if command.continuous_pipe {
            command
                .input
                .iter()
                .flat_map(|value| value.as_encoded_bytes())
                .copied()
                .collect()
        } else {
            command
                .input
                .join(OsStr::new(" "))
                .as_encoded_bytes()
                .to_vec()
        };
        let key = CacheKey {
            program: native.get_program().to_owned(),
            args: native.get_args().map(OsStr::to_owned).collect(),
            cwd,
            env,
            input: input.clone(),
        };
        let mut native = tokio::process::Command::from(native);
        native
            .kill_on_drop(true)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        native.process_group(0);
        #[cfg(windows)]
        native.creation_flags(windows_sys::Win32::System::Threading::CREATE_SUSPENDED);
        Ok(Self {
            command: native,
            input,
            key,
        })
    }
}
