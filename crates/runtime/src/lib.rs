//! Application composition, lifecycle, and top-level orchestration.

#[cfg(not(target_pointer_width = "64"))]
compile_error!("solarity-runtime supports only 64-bit application targets");

mod application;
mod configuration;
mod console;
mod event;
mod foundation;
mod input;
mod legal;
mod loading;
mod platform;
mod security;
mod telemetry;
mod time;

pub use application::{
    ApplicationError, ApplicationExitReason, ApplicationRunReport, ClientApplication,
    GameplaySession, GameplayUpdateError, StartupReport,
};
pub use configuration::{
    ConfigurationError, RuntimeConfiguration, WindowConfiguration, WindowMode,
};
pub use platform::{
    ButtonState, KeyCode, KeyModifiers, KeyStateEvent, MouseButton, MouseButtonEvent,
    MouseMotionEvent, MouseWheelDirection, MouseWheelEvent, PlatformError, PlatformEvent, ScanCode,
    TextEditingEvent, TextInputEvent, WindowEvent, WindowId,
};
