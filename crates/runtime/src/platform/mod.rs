//! SDL3-backed windowing and narrow Windows services still required by stock.
//!
//! The stock `OsCall.cpp`, `OsClipboard.cpp`, `OsIME.cpp`, `OsURLDownload.cpp`,
//! and related files establish the platform surface. Platform behavior enters
//! the rest of the client only through typed runtime events and services.

mod lcd;

mod blizzard_cursor;
mod calendar;
mod clock;
mod cursor;
mod event;
mod event_translation;
mod os_call;
mod os_clipboard;
mod os_ime;
mod os_secure_random;
mod os_url_download;
mod os_version_hash;
mod sdl_platform;
mod status;
mod thread_clock;
mod window_identity;

pub(crate) use calendar::realm_calendar_days;
pub(crate) use calendar::screenshot_timestamp;
pub(crate) use clock::client_milliseconds;
pub use event::{
    ButtonState, KeyCode, KeyModifiers, KeyStateEvent, MouseButton, MouseButtonEvent,
    MouseMotionEvent, MouseWheelDirection, MouseWheelEvent, PlatformEvent, ScanCode,
    TextEditingEvent, TextInputEvent, TimedPlatformEvent, WindowEvent, WindowId,
};
pub(crate) use sdl_platform::SdlPlatform;
pub use status::PlatformError;
pub(crate) use thread_clock::current_thread_cycles;
