//! Charged CPU cycles distinguish active work from long wall spans without clock conversion.

#![allow(unsafe_code)]

use crate::{Site, generation};
use std::time::Instant;

/// Coarse synchronous observation only; never retain across an await or move threads.
pub struct ThreadCycles {
    wall: &'static Site,
    cycles: &'static Site,
    available: &'static Site,
    epoch: u64,
    start: Option<(Instant, Option<u64>)>,
    /// Charged cycles are meaningful only on the thread that began this span.
    _thread_bound: std::marker::PhantomData<std::rc::Rc<()>>,
}

impl ThreadCycles {
    /// Disabled spans do not call the OS or read a clock. No handle is opened.
    pub fn new(wall: &'static Site, cycles: &'static Site, available: &'static Site) -> Self {
        let epoch = generation();
        Self {
            wall,
            cycles,
            available,
            epoch,
            start: (epoch != 0).then(|| (Instant::now(), current_cycles())),
            _thread_bound: std::marker::PhantomData,
        }
    }
}

impl Drop for ThreadCycles {
    fn drop(&mut self) {
        let Some((start, initial)) = self.start else {
            return;
        };
        if generation() != self.epoch {
            return;
        }
        let end = current_cycles();
        let elapsed = start.elapsed();
        self.wall.cpu_duration(self.epoch, "", elapsed);
        let charged = initial
            .zip(end)
            .and_then(|(first, last)| last.checked_sub(first));
        self.available
            .value_for_generation(self.epoch, u64::from(charged.is_some()));
        if let Some(charged) = charged {
            if elapsed.as_millis() >= 2 {
                self.cycles.event_value(self.epoch, charged);
            } else {
                self.cycles.value_for_generation(self.epoch, charged);
            }
        }
    }
}

/// The pseudo-handle needs no allocation, syscall to open a handle, or close.
fn current_cycles() -> Option<u64> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Threading::GetCurrentThread;
        use windows_sys::Win32::System::WindowsProgramming::QueryThreadCycleTime;
        let mut cycles = 0;
        // SAFETY: The current-thread pseudo-handle is valid and the output is writable.
        let ok = unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut cycles) };
        (ok != 0).then_some(cycles)
    }
    #[cfg(not(windows))]
    {
        None
    }
}
