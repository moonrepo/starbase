use crate::{shell_error::ShellError, shells::*};
use std::path::Path;
use std::str::FromStr;
use std::{env, fmt};
use tracing::{debug, instrument};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShellType {
    Ash,
    Bash,
    Elvish,
    Fish,
    Ion,
    Murex,
    Nu,
    PowerShell,
    Pwsh,
    Sh,
    Xonsh,
    Zsh,
}

impl ShellType {
    /// Return a list of all shell types.
    pub fn variants() -> Vec<Self> {
        vec![
            Self::Ash,
            Self::Bash,
            Self::Elvish,
            Self::Fish,
            Self::Ion,
            Self::Murex,
            Self::Nu,
            Self::PowerShell,
            Self::Pwsh,
            Self::Sh,
            Self::Xonsh,
            Self::Zsh,
        ]
    }

    /// Return a list of shell types for the current operating system.
    pub fn os_variants() -> Vec<Self> {
        #[cfg(windows)]
        {
            vec![
                Self::Bash,
                Self::Elvish,
                Self::Fish,
                Self::Murex,
                Self::Nu,
                Self::Xonsh,
                Self::PowerShell,
                Self::Pwsh,
            ]
        }

        #[cfg(unix)]
        Self::variants()
    }

    /// Detect the current shell by inspecting the `$SHELL` environment variable,
    /// and the parent process hierarchy.
    pub fn detect() -> Option<Self> {
        Self::try_detect().ok()
    }

    /// Detect the current shell by inspecting the `$SHELL` environment variable,
    /// and the parent process hierarchy. If no shell could be find, return a fallback.
    pub fn detect_with_fallback() -> Self {
        Self::detect().unwrap_or_default()
    }

    /// Detect the current shell by inspecting the `$SHELL` environment variable,
    /// and the parent process hierarchy, and return an error if not detected.
    #[instrument]
    pub fn try_detect() -> Result<Self, ShellError> {
        debug!("Attempting to detect the current shell");

        if let Ok(env_value) = env::var("SHELL") {
            if !env_value.is_empty() {
                debug!(
                    env = &env_value,
                    "Detecting from SHELL environment variable"
                );

                if let Some(shell) = parse_shell_from_file_path(&env_value) {
                    debug!("Detected {} shell", shell);

                    return Ok(shell);
                }
            }
        }

        debug!("Detecting from operating system");

        if let Some(shell) = os::detect() {
            debug!("Detected {} shell", shell);

            return Ok(shell);
        }

        debug!("Could not detect a shell!");

        Err(ShellError::CouldNotDetectShell)
    }

    /// Build a [`Shell`] instance from the current type.
    pub fn build(&self) -> BoxedShell {
        match self {
            Self::Ash => Box::new(Ash::new()),
            Self::Bash => Box::new(Bash::new()),
            Self::Elvish => Box::new(Elvish::new()),
            Self::Fish => Box::new(Fish::new()),
            Self::Ion => Box::new(Ion::new()),
            Self::Murex => Box::new(Murex::new()),
            Self::Nu => Box::new(Nu::new()),
            Self::PowerShell => Box::new(PowerShell::new()),
            Self::Pwsh => Box::new(Pwsh::new()),
            Self::Sh => Box::new(Sh::new()),
            Self::Xonsh => Box::new(Xonsh::new()),
            Self::Zsh => Box::new(Zsh::new()),
        }
    }
}

impl Default for ShellType {
    fn default() -> Self {
        let fallback = os::detect_fallback();

        debug!("Defaulting to {} shell", fallback);

        fallback
    }
}

impl fmt::Display for ShellType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Ash => "ash",
                Self::Bash => "bash",
                Self::Elvish => "elvish",
                Self::Fish => "fish",
                Self::Ion => "ion",
                Self::Murex => "murex",
                Self::Nu => "nu",
                Self::PowerShell => "powershell",
                Self::Pwsh => "pwsh",
                Self::Sh => "sh",
                Self::Xonsh => "xonsh",
                Self::Zsh => "zsh",
            }
        )
    }
}

impl FromStr for ShellType {
    type Err = ShellError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "ash" => Ok(ShellType::Ash),
            "bash" => Ok(ShellType::Bash),
            "elv" | "elvish" => Ok(ShellType::Elvish),
            "fish" => Ok(ShellType::Fish),
            "ion" => Ok(ShellType::Ion),
            "murex" => Ok(ShellType::Murex),
            "nu" | "nushell" => Ok(ShellType::Nu),
            "powershell" | "powershell_ise" => Ok(ShellType::PowerShell),
            "pwsh" => Ok(ShellType::Pwsh),
            "sh" => Ok(ShellType::Sh),
            "xonsh" | "xon.sh" => Ok(ShellType::Xonsh),
            "zsh" => Ok(ShellType::Zsh),
            _ => Err(ShellError::UnknownShell {
                name: value.to_owned(),
            }),
        }
    }
}

impl TryFrom<&str> for ShellType {
    type Error = ShellError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::from_str(value)
    }
}

impl TryFrom<String> for ShellType {
    type Error = ShellError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_str(&value)
    }
}

pub fn parse_shell_from_file_path<P: AsRef<Path>>(path: P) -> Option<ShellType> {
    // Remove trailing extensions (like `.exe`)
    let name = path.as_ref().file_stem()?.to_str()?;

    // Remove login shell leading `-`
    ShellType::from_str(name.strip_prefix('-').unwrap_or(name)).ok()
}

pub fn find_shell_on_path(shell: ShellType) -> bool {
    #[cfg(windows)]
    let file = format!("{}.exe", shell);

    #[cfg(unix)]
    let file = shell.to_string();

    let Some(path) = env::var_os("PATH") else {
        return false;
    };

    for dir in env::split_paths(&path) {
        let shell_path = dir.join(&file);

        if shell_path.exists() && shell_path.is_file() {
            return true;
        }
    }

    false
}

#[cfg(unix)]
mod os {
    use super::*;
    use std::io::BufRead;
    use std::process::{self, Command};
    use tracing::trace;

    pub struct ProcessStatus {
        ppid: Option<u32>,
        comm: String,
    }

    // PPID COMM
    //  635 -zsh
    pub fn detect_from_process_status(current_pid: u32) -> Option<ProcessStatus> {
        let output = Command::new("ps")
            .args(["-o", "ppid,comm"])
            .arg(current_pid.to_string())
            .output()
            .ok()?;

        let mut lines = output.stdout.lines();
        let line = lines.nth(1)?.ok()?;
        let mut parts = line.split_whitespace();

        match (parts.next(), parts.next()) {
            (Some(ppid), Some(comm)) => {
                let status = ProcessStatus {
                    ppid: ppid.parse().ok(),
                    comm: comm.to_owned(),
                };

                trace!(
                    pid = current_pid,
                    next_pid = &status.ppid,
                    comm = &status.comm,
                    "Running ps command to find shell"
                );

                Some(status)
            }
            _ => None,
        }
    }

    pub fn detect() -> Option<ShellType> {
        let mut pid = Some(process::id());
        let mut depth = 0;

        while let Some(current_pid) = pid {
            if depth > 10 || pid.is_some_and(|id| id == 0) {
                return None;
            }

            let Some(status) = detect_from_process_status(current_pid) else {
                break;
            };

            if let Some(shell) = parse_shell_from_file_path(status.comm) {
                return Some(shell);
            }

            pid = status.ppid;
            depth += 1;
        }

        None
    }

    pub fn detect_fallback() -> ShellType {
        if find_shell_on_path(ShellType::Bash) {
            ShellType::Bash
        } else {
            ShellType::Sh
        }
    }
}

#[cfg(windows)]
mod os {
    use super::*;
    use sysinfo::{ProcessesToUpdate, System, get_current_pid};
    use tracing::trace;

    pub fn detect() -> Option<ShellType> {
        let mut system = System::new();
        let mut pid = get_current_pid().ok();
        let mut depth = 0;

        while let Some(current_pid) = pid {
            if depth > 10 {
                return None;
            }

            system.refresh_processes(ProcessesToUpdate::Some(&[current_pid]), true);

            if let Some(process) = system.process(current_pid) {
                pid = process.parent();

                if let Some(exe_path) = process.exe() {
                    trace!(
                        pid = current_pid.as_u32(),
                        next_pid = pid.map(|p| p.as_u32()),
                        exe = ?exe_path,
                        "Inspecting process to find shell"
                    );

                    if let Some(shell) = parse_shell_from_file_path(exe_path) {
                        return Some(shell);
                    }
                }
            } else {
                break;
            }

            depth += 1;
        }

        None
    }

    pub fn detect_fallback() -> ShellType {
        if find_shell_on_path(ShellType::Pwsh) {
            ShellType::Pwsh
        } else {
            ShellType::PowerShell
        }
    }
}
