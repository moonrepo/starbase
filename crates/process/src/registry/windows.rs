//! Windows job ownership and suspended child startup.

use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use windows_sys::Win32::System::JobObjects::*;

// Tokio does not expose the primary thread handle. Since CREATE_SUSPENDED
// prevents the child from executing any user code, it has exactly one thread;
// find it by its pinned owner PID and resume only after successful assignment.
fn resume_child(pid: u32) -> io::Result<()> {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::*;
    use windows_sys::Win32::System::Threading::{
        GetProcessIdOfThread, OpenThread, ResumeThread, THREAD_QUERY_LIMITED_INFORMATION,
        THREAD_SUSPEND_RESUME,
    };
    // SAFETY: the flags request a system thread snapshot; the returned handle
    // is checked before transferring ownership to OwnedHandle.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let snapshot = unsafe { OwnedHandle::from_raw_handle(snapshot) };
    let mut entry = THREADENTRY32 {
        dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
        ..Default::default()
    };
    // SAFETY: entry has the required layout and initialized size.
    let mut found = unsafe { Thread32First(snapshot.as_raw_handle(), &mut entry) };
    while found != 0 {
        if entry.th32OwnerProcessID == pid {
            // SAFETY: open a handle before acting on the thread. Validate its
            // owner below in case an external kill caused thread ID reuse.
            let thread = unsafe {
                OpenThread(
                    THREAD_SUSPEND_RESUME | THREAD_QUERY_LIMITED_INFORMATION,
                    0,
                    entry.th32ThreadID,
                )
            };
            if thread.is_null() {
                return Err(io::Error::last_os_error());
            }
            let thread = unsafe { OwnedHandle::from_raw_handle(thread) };
            // SAFETY: the live handle pins the thread identity across both calls.
            if unsafe { GetProcessIdOfThread(thread.as_raw_handle()) } != pid {
                return Err(io::Error::other("suspended child thread identity changed"));
            }
            if unsafe { ResumeThread(thread.as_raw_handle()) } == u32::MAX {
                return Err(io::Error::last_os_error());
            }
            return Ok(());
        }
        found = unsafe { Thread32Next(snapshot.as_raw_handle(), &mut entry) };
    }
    Err(io::Error::other("suspended child thread was not found"))
}

pub(super) struct Job(OwnedHandle);

impl Job {
    pub(super) fn new(child: &tokio::process::Child) -> io::Result<Self> {
        // SAFETY: null security/name pointers select an unnamed, non-inherited
        // job. Every successfully returned handle is owned and closed once.
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = Self(unsafe { OwnedHandle::from_raw_handle(handle) });
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let child_handle = child
            .raw_handle()
            .ok_or_else(|| io::Error::other("child handle unavailable"))?;
        // SAFETY: handles are live; info has the required layout and size.
        if unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&info) as u32,
            )
        } == 0
            || unsafe { AssignProcessToJobObject(handle, child_handle) } == 0
        {
            return Err(io::Error::last_os_error());
        }
        resume_child(
            child
                .id()
                .ok_or_else(|| io::Error::other("child PID unavailable"))?,
        )?;
        Ok(job)
    }

    pub(super) fn terminate(&self) -> io::Result<()> {
        // SAFETY: this object owns the live job handle for the call's duration.
        if unsafe { TerminateJobObject(self.0.as_raw_handle(), 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}
