//! Main-thread SDL context, window, and event-pump ownership.

#![allow(unsafe_code)]

mod input;
mod owner;
mod wait;

use sdl3::video::Window;
use sdl3::{EventPump, Sdl, VideoSubsystem};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

struct NativeInput {
    wake: super::wakeup::WakeBridge,
    event_pump: EventPump,
}

/// Main-only borrowed access; never keeps SDL alive beyond the platform owner.
#[derive(Clone)]
pub(crate) struct NativeInputHandle(Weak<RefCell<NativeInput>>);

/// Exclusive owner of SDL objects whose lifecycle is constrained to one thread.
pub(crate) struct SdlPlatform {
    // Declaration order deliberately destroys the pump and window before their
    // subsystem handles and finally the process-level SDL context.
    input: Rc<RefCell<NativeInput>>,
    window: Window,
    total_physical_memory_bytes: u64,
    text_input_active: bool,
    mouse_free_look: bool,
    video: VideoSubsystem,
    sdl: Sdl,
}
