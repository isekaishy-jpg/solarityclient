//! Main-thread SDL context, window, and event-pump ownership.

use sdl3::video::Window;
use sdl3::{EventPump, Sdl, VideoSubsystem};

use crate::configuration::{WindowConfiguration, WindowMode};
use crate::platform::event_translation;
use crate::platform::{PlatformError, PlatformEvent, WindowId};

const CLIENT_WINDOW_TITLE: &str = "Solarity";

/// Exclusive owner of SDL objects whose lifecycle is constrained to one thread.
pub(crate) struct SdlPlatform {
    // Declaration order deliberately destroys the pump and window before their
    // subsystem handles and finally the process-level SDL context.
    event_pump: EventPump,
    window: Window,
    _video: VideoSubsystem,
    _sdl: Sdl,
}

impl SdlPlatform {
    /// Initializes a hidden Vulkan-capable window before renderer construction.
    ///
    /// The window remains hidden until a Vulkan swapchain can present a fully
    /// initialized frame; this avoids exposing undefined startup contents.
    pub(crate) fn start(configuration: WindowConfiguration) -> Result<Self, PlatformError> {
        let sdl = sdl3::init().map_err(|source| PlatformError::Initialize {
            message: source.to_string(),
        })?;
        let video = sdl.video().map_err(|source| PlatformError::Video {
            message: source.to_string(),
        })?;

        let mut builder = video.window(
            CLIENT_WINDOW_TITLE,
            configuration.width(),
            configuration.height(),
        );
        builder.vulkan().high_pixel_density().hidden();
        match configuration.mode() {
            WindowMode::Windowed => {
                builder.resizable();
            }
            WindowMode::Fullscreen => {
                builder.fullscreen();
            }
        }
        let window = builder.build().map_err(|source| PlatformError::Window {
            message: source.to_string(),
        })?;
        let event_pump = sdl
            .event_pump()
            .map_err(|source| PlatformError::EventPump {
                message: source.to_string(),
            })?;

        Ok(Self {
            event_pump,
            window,
            _video: video,
            _sdl: sdl,
        })
    }

    /// Polls until it finds one admitted client event or exhausts SDL's queue.
    pub(crate) fn poll_event(&mut self) -> Option<PlatformEvent> {
        while let Some(event) = self.event_pump.poll_event() {
            if let Some(event) = event_translation::translate(event) {
                return Some(event);
            }
        }
        None
    }

    /// Returns the SDL identifier used to reject or route window-scoped work.
    pub(crate) fn window_id(&self) -> WindowId {
        WindowId::from_sdl(self.window.id())
    }

    /// Returns the logical size used by UI coordinate calculations.
    pub(crate) fn logical_extent(&self) -> (u32, u32) {
        self.window.size()
    }

    /// Returns the physical drawable size required for swapchain construction.
    pub(crate) fn pixel_extent(&self) -> (u32, u32) {
        self.window.size_in_pixels()
    }
}
