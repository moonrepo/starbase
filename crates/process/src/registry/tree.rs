//! Best effort enumeration and signalling of process trees.

use crate::signal::SignalType;
#[cfg(not(target_os = "macos"))]
use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;
use std::io;
use tracing::debug;

pub(super) fn kill_descendants(pids: &[u32], signal: SignalType) {
    for pid in pids {
        if let Err(error) = kill(*pid, signal) {
            debug!(pid, %error, "Failed to signal descendant process");
        }
    }
}

/// Return every descendant of `pid` (children, grandchildren, and so
/// on), parents before their children. Failures yield an empty list.
pub fn descendants(pid: u32) -> Vec<u32> {
    let mut children_of = imp::children_lookup();
    let mut seen = FxHashSet::default();
    let mut queue = vec![pid];
    let mut found = vec![];

    seen.insert(pid);

    while let Some(parent) = queue.pop() {
        for child in children_of(parent) {
            if seen.insert(child) {
                found.push(child);
                queue.push(child);
            }
        }
    }

    found
}

/// Signal a process that isn't one of our children by pid.
pub fn kill(pid: u32, signal: SignalType) -> io::Result<()> {
    imp::kill(pid, signal)
}

/// Build a parent -> children lookup from a `(pid, ppid)` list.
#[cfg(not(target_os = "macos"))]
fn lookup_from_pairs(pairs: Vec<(u32, u32)>) -> impl FnMut(u32) -> Vec<u32> {
    let mut map: FxHashMap<u32, Vec<u32>> = FxHashMap::default();

    for (pid, ppid) in pairs {
        map.entry(ppid).or_default().push(pid);
    }

    move |ppid| map.remove(&ppid).unwrap_or_default()
}

#[cfg(target_os = "linux")]
mod imp {
    use super::*;

    pub fn children_lookup() -> impl FnMut(u32) -> Vec<u32> {
        let mut pairs = vec![];

        if let Ok(dir) = std::fs::read_dir("/proc") {
            for entry in dir.flatten() {
                let Some(pid) = entry
                    .file_name()
                    .to_str()
                    .and_then(|name| name.parse::<u32>().ok())
                else {
                    continue;
                };

                let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
                    continue;
                };

                // `pid (comm) state ppid ...`, where comm may contain
                // spaces and parentheses, so split on the last `)`
                let Some(ppid) = stat
                    .rsplit_once(')')
                    .and_then(|(_, rest)| rest.split_whitespace().nth(1))
                    .and_then(|ppid| ppid.parse::<u32>().ok())
                else {
                    continue;
                };

                pairs.push((pid, ppid));
            }
        }

        lookup_from_pairs(pairs)
    }

    pub fn kill(pid: u32, signal: SignalType) -> io::Result<()> {
        crate::signal::kill(pid, signal)
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use std::ffi::c_int;
    use std::mem::size_of;

    pub fn children_lookup() -> impl FnMut(u32) -> Vec<u32> {
        |ppid| {
            let mut capacity = 64;

            loop {
                let mut buffer = vec![0 as libc::pid_t; capacity];
                let size = (capacity * size_of::<libc::pid_t>()) as c_int;

                // Returns the number of pids written
                let written = unsafe {
                    libc::proc_listchildpids(ppid as libc::pid_t, buffer.as_mut_ptr().cast(), size)
                };

                if written <= 0 {
                    return vec![];
                }

                let count = written as usize;

                // A full buffer may have been truncated
                if count >= capacity && capacity < (1 << 16) {
                    capacity *= 2;
                    continue;
                }

                buffer.truncate(count);

                return buffer
                    .into_iter()
                    .filter(|pid| *pid > 0)
                    .map(|pid| pid as u32)
                    .collect();
            }
        }
    }

    pub fn kill(pid: u32, signal: SignalType) -> io::Result<()> {
        crate::signal::kill(pid, signal)
    }
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
mod imp {
    use super::*;

    pub fn children_lookup() -> impl FnMut(u32) -> Vec<u32> {
        let output = std::process::Command::new("ps")
            .args(["-A", "-o", "pid=,ppid="])
            .output();

        let pairs = match output {
            Ok(output) => String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter_map(|line| {
                    let mut parts = line.split_whitespace();
                    let pid = parts.next()?.parse().ok()?;
                    let ppid = parts.next()?.parse().ok()?;
                    Some((pid, ppid))
                })
                .collect(),
            Err(_) => vec![],
        };

        lookup_from_pairs(pairs)
    }

    pub fn kill(pid: u32, signal: SignalType) -> io::Result<()> {
        crate::signal::kill(pid, signal)
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32, Process32First, Process32Next, TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};

    const ERROR_ACCESS_DENIED: i32 = 5;
    const ERROR_INVALID_PARAMETER: i32 = 87;

    pub fn children_lookup() -> impl FnMut(u32) -> Vec<u32> {
        let mut pairs = vec![];
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };

        if snapshot != INVALID_HANDLE_VALUE {
            let mut entry: PROCESSENTRY32 = unsafe { std::mem::zeroed() };
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32>() as u32;

            if unsafe { Process32First(snapshot, &mut entry) } != 0 {
                loop {
                    pairs.push((entry.th32ProcessID, entry.th32ParentProcessID));

                    if unsafe { Process32Next(snapshot, &mut entry) } == 0 {
                        break;
                    }
                }
            }

            unsafe { CloseHandle(snapshot) };
        }

        lookup_from_pairs(pairs)
    }

    /// `Interrupt` is a no-op, as `CTRL-C` can't be targeted at a
    /// process; everything else terminates it. A process that no longer
    /// exists is treated as already dead.
    pub fn kill(pid: u32, signal: SignalType) -> io::Result<()> {
        if matches!(signal, SignalType::Interrupt) {
            return Ok(());
        }

        let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };

        if handle.is_null() {
            let error = io::Error::last_os_error();

            return if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER) {
                Ok(())
            } else {
                Err(error)
            };
        }

        let result = unsafe { TerminateProcess(handle, 1) };
        let error = io::Error::last_os_error();

        unsafe { CloseHandle(handle) };

        if result == 0 && error.raw_os_error() != Some(ERROR_ACCESS_DENIED) {
            return Err(error);
        }

        Ok(())
    }
}
