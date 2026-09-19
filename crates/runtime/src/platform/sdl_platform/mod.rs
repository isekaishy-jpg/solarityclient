//! Main-thread SDL context, window, and event-pump ownership.

#![allow(unsafe_code)]

mod input;
mod owner;
mod wait;

use sdl3::video::Window;
use sdl3::{EventPump, Sdl, VideoSubsystem};

/// Exclusive owner of SDL objects whose lifecycle is constrained to one thread.
pub(crate) struct SdlPlatform {
    wake: super::wakeup::WakeBridge,
    // Declaration order deliberately destroys the pump and window before their
    // subsystem handles and finally the process-level SDL context.
    event_pump: EventPump,
    window: Window,
    total_physical_memory_bytes: u64,
    text_input_active: bool,
    mouse_free_look: bool,
    video: VideoSubsystem,
    sdl: Sdl,
}
