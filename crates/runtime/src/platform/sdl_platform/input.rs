//! Ordered SDL event translation and admitted text/mouse input modes.

use super::SdlPlatform;
use crate::platform::{PlatformError, PlatformEvent, event_translation};
use sdl3::video::WindowFlags;

impl SdlPlatform {
    /// Polls until it finds one admitted client event or exhausts SDL's queue.
    pub(crate) fn poll_event(&mut self) -> Option<crate::platform::TimedPlatformEvent> {
        loop {
            let event = {
                let _profile = solarity_profiling::profile!("platform.sdl.poll");
                self.event_pump.poll_event()
            };
            let event = event?;
            if let Some(event) = event_translation::translate(event) {
                if let PlatformEvent::Window {
                    window_id,
                    event: window_event,
                } = &event.event
                    && *window_id == self.window_id()
                {
                    tracing::info!(
                        event = ?window_event,
                        position = ?self.window.position(),
                        logical_extent = ?self.logical_extent(),
                        pixel_extent = ?self.pixel_extent(),
                        flags = ?WindowFlags::from(self.window.window_flags()),
                        "primary SDL window lifecycle event"
                    );
                }
                return Some(event);
            }
        }
    }

    /// Starts or stops SDL text/IME delivery for the primary window.
    pub(crate) fn set_text_input_active(&mut self, active: bool) {
        if self.text_input_active == active {
            return;
        }
        let text_input = self.video.text_input();
        if active {
            text_input.start(&self.window);
        } else {
            text_input.stop(&self.window);
        }
        self.text_input_active = active;
    }

    /// Relative mode hides and confines the cursor during an admitted camera
    /// gesture; SDL releases confinement when this window loses focus.
    pub(crate) fn set_mouse_free_look(&mut self, enabled: bool) -> Result<(), PlatformError> {
        if enabled == self.mouse_free_look {
            return Ok(());
        }
        let mouse = self.sdl.mouse();
        mouse.set_relative_mouse_mode(&self.window, enabled);
        if mouse.relative_mouse_mode(&self.window) != enabled {
            return Err(PlatformError::RelativeMouse {
                message: sdl3::get_error().to_string(),
            });
        }
        self.mouse_free_look = enabled;
        Ok(())
    }
}
