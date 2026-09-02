//! Process identity that keeps the client independent in the Windows shell.

#![allow(unsafe_code)]

use crate::platform::PlatformError;
use sdl3::video::Window;

const SDL_APP_NAME: &std::ffi::CStr = c"Solarity";
const SDL_APP_VERSION: &std::ffi::CStr = c"0.1.0";
const SDL_APP_IDENTIFIER: &std::ffi::CStr = c"com.solarity.client";

/// Establishes SDL metadata and a stable Windows AppUserModelID before video starts.
///
/// A process launched by the PowerShell test harness otherwise inherits only an
/// implicit shell identity. Windows may then place it in a taskbar/Snap group with
/// unrelated launcher windows. The explicit process identity must precede creation
/// of the primary HWND so that its caption controls operate on Solarity alone.
pub(super) fn establish() -> Result<(), PlatformError> {
    // SAFETY: All three pointers refer to static, NUL-terminated C strings.
    let metadata_set = unsafe {
        sdl3::sys::init::SDL_SetAppMetadata(
            SDL_APP_NAME.as_ptr(),
            SDL_APP_VERSION.as_ptr(),
            SDL_APP_IDENTIFIER.as_ptr(),
        )
    };
    if !metadata_set {
        return Err(PlatformError::Initialize {
            message: sdl3::get_error().to_string(),
        });
    }
    establish_native_identity()
}

/// Marks the SDL-created HWND as an independent primary application window.
#[cfg(windows)]
pub(super) fn mark_primary_window(window: &Window) -> Result<(), PlatformError> {
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongPtrW, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        SWP_NOZORDER, SetWindowLongPtrW, SetWindowPos, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
    };

    // SAFETY: `window` owns a live SDL window. SDL's documented Win32 property
    // contains its HWND for the complete lifetime of this owner.
    let hwnd = unsafe {
        let properties = sdl3::sys::video::SDL_GetWindowProperties(window.raw());
        if properties == 0 {
            return Err(PlatformError::ApplicationWindowStyle {
                code: GetLastError(),
            });
        }
        sdl3::sys::properties::SDL_GetPointerProperty(
            properties,
            sdl3::sys::video::SDL_PROP_WINDOW_WIN32_HWND_POINTER,
            std::ptr::null_mut(),
        )
    };
    if hwnd.is_null() {
        // SDL did not publish the platform handle. Its own error is not a Win32
        // code, so retain a stable nonzero sentinel at this native boundary.
        return Err(PlatformError::ApplicationWindowStyle { code: u32::MAX });
    }

    // SAFETY: `hwnd` is the live primary window retrieved above. Updating only
    // its extended style preserves all SDL-managed ordinary window state.
    let previous_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
    let standalone_style =
        (previous_style | WS_EX_APPWINDOW as isize) & !(WS_EX_TOOLWINDOW as isize);
    // SAFETY: The same live HWND is retained by `window`; the style value changes
    // only shell classification and leaves sizing/caption flags untouched.
    let replaced_style = unsafe { SetWindowLongPtrW(hwnd, GWL_EXSTYLE, standalone_style) };
    if replaced_style == 0 && previous_style != 0 {
        // A nonzero previous style must be returned on success.
        return Err(PlatformError::ApplicationWindowStyle {
            // SAFETY: Read immediately after the failed Win32 call.
            code: unsafe { GetLastError() },
        });
    }
    // SAFETY: This nonmoving, nonactivating frame refresh commits the extended
    // shell style without altering position, size, focus, or z-order.
    let refreshed = unsafe {
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
        )
    };
    if refreshed == 0 {
        return Err(PlatformError::ApplicationWindowStyle {
            // SAFETY: Read immediately after the failed Win32 call.
            code: unsafe { GetLastError() },
        });
    }
    Ok(())
}

/// Non-Windows platforms already own their native application classification.
#[cfg(not(windows))]
pub(super) const fn mark_primary_window(_window: &Window) -> Result<(), PlatformError> {
    Ok(())
}

#[cfg(windows)]
fn establish_native_identity() -> Result<(), PlatformError> {
    const WINDOWS_APP_ID: &[u16] = &[
        b'S' as u16,
        b'o' as u16,
        b'l' as u16,
        b'a' as u16,
        b'r' as u16,
        b'i' as u16,
        b't' as u16,
        b'y' as u16,
        b'.' as u16,
        b'C' as u16,
        b'l' as u16,
        b'i' as u16,
        b'e' as u16,
        b'n' as u16,
        b't' as u16,
        0,
    ];
    // SAFETY: `WINDOWS_APP_ID` is a static NUL-terminated UTF-16 string and the
    // shell copies it during this process-wide call.
    let result = unsafe {
        windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(
            WINDOWS_APP_ID.as_ptr(),
        )
    };
    if result >= 0 {
        return Ok(());
    }
    Err(PlatformError::ApplicationIdentity {
        code: result as u32,
    })
}

#[cfg(not(windows))]
const fn establish_native_identity() -> Result<(), PlatformError> {
    Ok(())
}
