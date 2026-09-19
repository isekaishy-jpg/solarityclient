//! Win32 event/timer/message wait; SDL remains the only input dispatcher.

#![allow(unsafe_code)]

use std::ffi::c_void;
use std::marker::PhantomData;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

use solarity_cpu::CoordinatorNotifier;
use windows_sys::Win32::Foundation::{GetLastError, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Threading::{
    CREATE_EVENT_MANUAL_RESET, CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, CreateEventExW,
    CreateWaitableTimerExW, EVENT_MODIFY_STATE, ResetEvent,
    SYNCHRONIZATION_SYNCHRONIZE as SYNCHRONIZE, SetEvent, SetWaitableTimer, TIMER_MODIFY_STATE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, QS_ALLINPUT,
};

use super::{MAINTENANCE_INTERVAL, SignalAction, WakeReason, WakeSequence, WakeTicket};
use crate::platform::PlatformError;

/// Shared signal owns its handle until the final worker/watch producer leaves.
struct NativeSignal {
    event: OwnedHandle,
    sequence: WakeSequence,
    fault: AtomicU32,
}

impl NativeSignal {
    /// Creates an unnamed, non-inheritable notification event.
    fn new() -> Result<Arc<Self>, PlatformError> {
        // SAFETY: null security/name select an unnamed non-inheritable event.
        let event = unsafe {
            CreateEventExW(
                std::ptr::null(),
                std::ptr::null(),
                CREATE_EVENT_MANUAL_RESET,
                SYNCHRONIZE | EVENT_MODIFY_STATE,
            )
        };
        if event.is_null() {
            return Err(native_error("create completion event"));
        }
        Ok(Arc::new(Self {
            // SAFETY: successful creation transfers one unique owned handle.
            event: unsafe { OwnedHandle::from_raw_handle(event) },
            sequence: WakeSequence::default(),
            fault: AtomicU32::new(0),
        }))
    }

    /// Surfaces producer failures on the coordinator without worker-side logging.
    fn check(&self) -> Result<(), PlatformError> {
        let code = self.fault.load(Ordering::Acquire);
        if code == 0 {
            Ok(())
        } else {
            Err(PlatformError::NativeWait {
                operation: "signal completion event",
                code,
            })
        }
    }
}

impl CoordinatorNotifier for NativeSignal {
    fn notify(&self) {
        match self.sequence.notify() {
            SignalAction::Coalesced => return,
            SignalAction::Wake => {}
            SignalAction::Exhausted => {
                self.fault.store(534, Ordering::Release);
            }
        }
        // SAFETY: this Arc pins the event through the entire signal operation.
        if unsafe { SetEvent(self.event.as_raw_handle()) } == 0 {
            // SAFETY: GetLastError has no preconditions and follows the failure.
            self.fault
                .store(unsafe { GetLastError() }.max(1), Ordering::Release);
        }
    }
}

/// Watch removal synchronizes with callbacks before releasing its signal Arc.
struct SdlWatch {
    signal: Arc<NativeSignal>,
}

impl SdlWatch {
    /// Adds an allocation-free notification watch, without translating events.
    fn new(signal: Arc<NativeSignal>) -> Result<Self, PlatformError> {
        // SAFETY: Arc allocation stays stable through removal. SDL invokes only
        // shared atomic/native operations; no &mut alias or gameplay callback.
        let added = unsafe {
            sdl3::sys::events::SDL_AddEventWatch(
                Some(watch),
                Arc::as_ptr(&signal).cast_mut().cast(),
            )
        };
        if !added {
            return Err(PlatformError::EventWatch {
                message: sdl3::get_error().to_string(),
            });
        }
        Ok(Self { signal })
    }
}

impl Drop for SdlWatch {
    fn drop(&mut self) {
        // SAFETY: pinned SDL_eventwatch.c locks the watcher list during dispatch
        // and removal. Removal runs outside a callback and finishes before the
        // Arc/userdata is released; the SDL subsystem still exists here.
        unsafe {
            sdl3::sys::events::SDL_RemoveEventWatch(
                Some(watch),
                Arc::as_ptr(&self.signal).cast_mut().cast(),
            );
        }
    }
}

/// SDL watches are advisory: SDL invokes them before adding an event to its queue.
extern "C" fn watch(data: *mut c_void, _event: *mut sdl3::sys::events::SDL_Event) -> bool {
    // SAFETY: SdlWatch pins this immutable allocation through callback removal.
    let signal = unsafe { &*data.cast::<NativeSignal>() };
    signal.notify();
    true
}

/// Single-thread wait owner; its producer signal has an independently pinned lifetime.
pub(in crate::platform) struct WakeBridge {
    _watch: SdlWatch,
    signal: Arc<NativeSignal>,
    timer: OwnedHandle,
    _main_thread: PhantomData<Rc<()>>,
}

impl WakeBridge {
    /// Initializes the required high-resolution capability before worker admission.
    pub(in crate::platform) fn new() -> Result<Self, PlatformError> {
        let signal = NativeSignal::new()?;
        // SAFETY: null attributes/name; no callback or inherited handle is created.
        let timer = unsafe {
            CreateWaitableTimerExW(
                std::ptr::null(),
                std::ptr::null(),
                CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                SYNCHRONIZE | TIMER_MODIFY_STATE,
            )
        };
        if timer.is_null() {
            return Err(native_error("create high-resolution deadline timer"));
        }
        // SAFETY: successful creation transfers a unique handle to this owner.
        let timer = unsafe { OwnedHandle::from_raw_handle(timer) };
        let watch = SdlWatch::new(Arc::clone(&signal))?;
        Ok(Self {
            _watch: watch,
            signal,
            timer,
            _main_thread: PhantomData,
        })
    }

    /// Polls latched native faults even when the frame has no remaining wait time.
    pub(in crate::platform) fn check(&self) -> Result<(), PlatformError> {
        self.signal.check()
    }

    /// Supplies only the wake interface to CPU and external producers.
    pub(in crate::platform) fn notifier(&self) -> Arc<dyn CoordinatorNotifier> {
        self.signal.clone()
    }

    /// Records the generation before the caller's final SDL/readiness check.
    pub(in crate::platform) fn observe(&self) -> WakeTicket {
        self.signal.sequence.observe()
    }

    /// Waits for a published completion, window input, or a finite deadline.
    pub(in crate::platform) fn wait(
        &mut self,
        ticket: WakeTicket,
        deadline: Instant,
    ) -> Result<WakeReason, PlatformError> {
        self.signal.check()?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(WakeReason::Deadline);
        }
        // Reset before CAS: any notification racing with reset invalidates the
        // ticket, or observes the successfully armed flag and signals afterward.
        // SAFETY: this owner pins the live event throughout reset and wait.
        if unsafe { ResetEvent(self.signal.event.as_raw_handle()) } == 0 {
            return Err(native_error("reset completion event"));
        }
        if !self.signal.sequence.arm(ticket) {
            return Ok(WakeReason::Completion);
        }
        let _armed = ArmedWait(&self.signal.sequence);
        let interval = remaining.min(MAINTENANCE_INTERVAL);
        let ticks = interval.as_nanos().div_ceil(100) as i64;
        let due = -ticks;
        // SAFETY: live timer, valid relative due-time pointer, one-shot with no APC.
        if unsafe {
            SetWaitableTimer(
                self.timer.as_raw_handle(),
                &due,
                0,
                None,
                std::ptr::null(),
                0,
            )
        } == 0
        {
            return Err(native_error("arm deadline timer"));
        }
        let handles = [
            self.signal.event.as_raw_handle(),
            self.timer.as_raw_handle(),
        ];
        // Timer carries sub-millisecond precision. The finite timeout also bounds
        // signal faults and SDL's pre-publication watch/queue insertion window.
        let timeout_ms = interval.as_millis() as u32 + 1;
        let _profile = solarity_profiling::profile!("platform.coordinator.wait");
        // SAFETY: both distinct handles are pinned until the call returns; no
        // wait-all, APC, second input dispatcher, or infinite timeout is used.
        let result = unsafe {
            MsgWaitForMultipleObjectsEx(
                2,
                handles.as_ptr(),
                timeout_ms,
                QS_ALLINPUT,
                MWMO_INPUTAVAILABLE,
            )
        };
        self.signal.check()?;
        match result {
            WAIT_OBJECT_0 => Ok(WakeReason::Completion),
            value if value == WAIT_OBJECT_0 + 2 => Ok(WakeReason::Input),
            value if value == WAIT_OBJECT_0 + 1 || value == WAIT_TIMEOUT => {
                Ok(if Instant::now() >= deadline {
                    WakeReason::Deadline
                } else {
                    WakeReason::Maintenance
                })
            }
            WAIT_FAILED => Err(native_error("wait for coordinator work")),
            code => Err(PlatformError::NativeWait {
                operation: "unexpected coordinator wait result",
                code,
            }),
        }
    }
}

/// Releases the armed flag on every return, including native failures.
struct ArmedWait<'a>(&'a WakeSequence);
impl Drop for ArmedWait<'_> {
    fn drop(&mut self) {
        self.0.disarm();
    }
}

/// Captures the native failure immediately without substituting a sleep/spin path.
fn native_error(operation: &'static str) -> PlatformError {
    // SAFETY: GetLastError has no pointer or lifetime preconditions.
    PlatformError::NativeWait {
        operation,
        code: unsafe { GetLastError() },
    }
}

#[cfg(test)]
#[path = "../../../tests/platform/wakeup_native.rs"]
mod tests;
