//! SDL3-backed windowing and narrow Windows services still required by stock.
//!
//! The stock `OsCall.cpp`, `OsClipboard.cpp`, `OsIME.cpp`, `OsURLDownload.cpp`,
//! and related files establish the platform surface. Platform behavior enters
//! the rest of the client only through typed runtime events and services.

mod lcd;

mod blizzard_cursor;
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
mod window_identity;

pub use event::{
    ButtonState, KeyCode, KeyModifiers, KeyStateEvent, MouseButton, MouseButtonEvent,
    MouseMotionEvent, MouseWheelDirection, MouseWheelEvent, PlatformEvent, ScanCode,
    TextEditingEvent, TextInputEvent, WindowEvent, WindowId,
};
pub(crate) use sdl_platform::SdlPlatform;
pub use status::PlatformError;
