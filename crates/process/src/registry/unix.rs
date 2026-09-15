//! Observe Unix child exits without reaping and inspect macOS process groups.

use super::lock;
use super::process::Process;
use std::io;

#[cfg(target_vendor = "apple")]
pub(super) fn apple_group_has_only_zombies(pgid: u32) -> io::Result<bool> {
    const PROC_PGRP_ONLY: u32 = 2;
    let mut pids = vec![0_i32; 32];
    loop {
        let bytes =
            i32::try_from(std::mem::size_of_val(pids.as_slice())).map_err(io::Error::other)?;
        // SAFETY: the buffer is writable for `bytes` bytes. This only inspects
        // the private process group while its unreaped leader pins the group ID.
        let written =
            unsafe { libc::proc_listpids(PROC_PGRP_ONLY, pgid, pids.as_mut_ptr().cast(), bytes) };
        if written <= 0 {
            return Err(io::Error::last_os_error());
        }
        if written == bytes {
            pids.resize(pids.len() * 2, 0);
            continue;
        }
        pids.truncate(written as usize / std::mem::size_of::<i32>());
        break;
    }
    for pid in pids {
        // SAFETY: a zeroed proc_bsdinfo and its exact size are passed to libproc.
        let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of_val(&info) as i32;
        let read = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDTBSDINFO,
                0,
                (&mut info as *mut libc::proc_bsdinfo).cast(),
                size,
            )
        };
        if read != size {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::ESRCH) {
                continue;
            }
            return Err(error);
        }
        if info.pbi_pgid == pgid && info.pbi_status != libc::SZOMB {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(super) async fn wait_unreaped(
    process: &Process,
    signals: &mut tokio::signal::unix::Signal,
) -> io::Result<()> {
    loop {
        match has_exited_unreaped(process.child.id()) {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => {
                // Someone outside this API reaped the PID. Never signal it again.
                if error.raw_os_error() == Some(libc::ECHILD) {
                    *lock(&process.active) = false;
                }
                return Err(error);
            }
        }
        if signals.recv().await.is_none() {
            return Err(io::Error::other("SIGCHLD listener closed"));
        }
    }
}

fn has_exited_unreaped(pid: u32) -> io::Result<bool> {
    // SAFETY: zero is a valid initial siginfo_t representation; waitid writes it
    // on success. WNOWAIT observes exit without releasing the PID. Only this
    // supervisor owns the child's wait API, including during signalling.
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    let result = unsafe {
        libc::waitid(
            libc::P_PID,
            pid as libc::id_t,
            &mut info,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    if result == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: waitid initialized info and si_pid is valid for this event.
    Ok(unsafe { info.si_pid() } != 0)
}
