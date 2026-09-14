//! Read-only process/thread counters run exclusively on the diagnostic writer.

#![allow(unsafe_code)]

use std::io::{self, Write};
use std::mem::size_of;

use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, GetCurrentThreadId, GetThreadTimes, OpenThread,
    THREAD_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::System::WindowsProgramming::QueryThreadCycleTime;

/// Closes real owned handles; pseudo process handles never enter this wrapper.
struct OwnedHandle(HANDLE);
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: Construction follows successful OpenThread or snapshot creation.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

/// Captured only during a thread's first enabled probe, not per measurement.
pub(crate) fn thread_id() -> u32 {
    // SAFETY: This process-local query has no pointer or lifetime preconditions.
    unsafe { GetCurrentThreadId() }
}

/// Converts Windows's cumulative 100 ns counters without floating-point loss.
fn ticks(value: FILETIME) -> u64 {
    (u64::from(value.dwHighDateTime) << 32) | u64::from(value.dwLowDateTime)
}

/// Records CPU counters for all process threads, including uninstrumented native code.
pub(crate) fn write_snapshot(output: &mut impl Write, seconds: f64) -> io::Result<()> {
    // SAFETY: Zero is valid initialization for this output-only C counter struct.
    let mut memory: PROCESS_MEMORY_COUNTERS_EX = unsafe { std::mem::zeroed() };
    memory.cb = size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
    // SAFETY: Current-process pseudo handle and correctly sized live output storage.
    let memory_ok = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            (&mut memory as *mut PROCESS_MEMORY_COUNTERS_EX).cast(),
            memory.cb,
        )
    };
    // SAFETY: Queries the current process without external state changes.
    let process = unsafe { GetCurrentProcessId() };
    // SAFETY: Creates an owned read-only snapshot, checked before wrapping.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let snapshot = OwnedHandle(snapshot);
    // SAFETY: THREADENTRY32 permits zero initialization before setting dwSize.
    let mut entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
    entry.dwSize = size_of::<THREADENTRY32>() as u32;
    // SAFETY: Snapshot is live; output storage has the documented size field.
    let mut valid = unsafe { Thread32First(snapshot.0, &mut entry) };
    let mut unavailable = 0_u64;
    while valid != 0 {
        if entry.th32OwnerProcessID == process {
            // SAFETY: Queries only; a racing thread exit is represented as unavailable.
            let handle =
                unsafe { OpenThread(THREAD_QUERY_LIMITED_INFORMATION, 0, entry.th32ThreadID) };
            if handle.is_null() {
                unavailable += 1;
            } else {
                let handle = OwnedHandle(handle);
                let mut created = FILETIME {
                    dwLowDateTime: 0,
                    dwHighDateTime: 0,
                };
                let mut exited = created;
                let mut kernel = created;
                let mut user = created;
                let mut cycles = 0;
                // SAFETY: The owned query handle and all output pointers remain live.
                let ok = unsafe {
                    GetThreadTimes(handle.0, &mut created, &mut exited, &mut kernel, &mut user)
                };
                // SAFETY: Same owned handle, with valid u64 output storage.
                let cycles_ok = unsafe { QueryThreadCycleTime(handle.0, &mut cycles) };
                if ok != 0 {
                    writeln!(
                        output,
                        "{seconds:.6},thread,{process},{},{},{},{cycles},{cycles_ok},0,0",
                        entry.th32ThreadID,
                        ticks(kernel),
                        ticks(user)
                    )?;
                } else {
                    unavailable += 1;
                }
            }
        }
        // SAFETY: Snapshot and sized entry remain valid throughout enumeration.
        valid = unsafe { Thread32Next(snapshot.0, &mut entry) };
    }
    writeln!(
        output,
        "{seconds:.6},memory,{process},0,0,0,0,{memory_ok},{},{}",
        memory.WorkingSetSize, memory.PrivateUsage
    )?;
    writeln!(
        output,
        "{seconds:.6},unavailable_threads,{process},0,0,0,{unavailable},0,0,0"
    )
}
