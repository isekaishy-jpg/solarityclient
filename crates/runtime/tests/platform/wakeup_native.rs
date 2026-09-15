//! Private native fault and already-observed Windows input coverage.

use std::error::Error;
use std::time::{Duration, Instant};

use super::{PlatformError, WakeBridge, WakeReason};
use crate::test_support::SDL_TEST_LOCK;

#[test]
fn latched_signal_fault_surfaces_even_when_deadline_already_expired() -> Result<(), Box<dyn Error>>
{
    let _guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL lock poisoned")?;
    let sdl = sdl3::init()?;
    let _events = sdl.event()?;
    let mut bridge = WakeBridge::new()?;
    bridge
        .signal
        .fault
        .store(1234, std::sync::atomic::Ordering::Release);
    assert!(matches!(
        bridge.check(),
        Err(PlatformError::NativeWait { code: 1234, .. })
    ));
    assert!(matches!(
        bridge.wait(bridge.observe(), Instant::now()),
        Err(PlatformError::NativeWait { code: 1234, .. })
    ));
    Ok(())
}

#[test]
#[allow(unsafe_code)]
fn previously_observed_window_message_still_wakes_native_wait() -> Result<(), Box<dyn Error>> {
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MSG, PM_NOREMOVE, PM_REMOVE, PeekMessageW, PostThreadMessageW, WM_APP,
    };

    let _guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL lock poisoned")?;
    let sdl = sdl3::init()?;
    let _events = sdl.event()?;
    let mut bridge = WakeBridge::new()?;
    let mut message = MSG::default();
    // SAFETY: writable message and null window target this test thread's queue.
    unsafe {
        PeekMessageW(
            &mut message,
            std::ptr::null_mut(),
            WM_APP,
            WM_APP,
            PM_NOREMOVE,
        );
    }
    // SAFETY: creates a pointer-free application message for this same thread.
    assert_ne!(
        unsafe { PostThreadMessageW(GetCurrentThreadId(), WM_APP, 0, 0) },
        0
    );
    // SAFETY: observe without removing; MWMO_INPUTAVAILABLE must still see it.
    assert_ne!(
        unsafe {
            PeekMessageW(
                &mut message,
                std::ptr::null_mut(),
                WM_APP,
                WM_APP,
                PM_NOREMOVE,
            )
        },
        0
    );
    let result = bridge.wait(bridge.observe(), Instant::now() + Duration::from_secs(1));
    // SAFETY: remove only the fixture's WM_APP after the wait, even on failure.
    unsafe {
        PeekMessageW(
            &mut message,
            std::ptr::null_mut(),
            WM_APP,
            WM_APP,
            PM_REMOVE,
        );
    }
    assert_eq!(result?, WakeReason::Input);
    Ok(())
}
