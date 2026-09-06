//! Main-thread SDL context, window, and event-pump ownership.

#![allow(unsafe_code)]

use sdl3::video::{Window, WindowFlags};
use sdl3::{EventPump, Sdl, VideoSubsystem};

use crate::configuration::{WindowConfiguration, WindowMode};
use crate::platform::event_translation;
use crate::platform::window_identity;
use crate::platform::{PlatformError, PlatformEvent, WindowId};

/// Exclusive owner of SDL objects whose lifecycle is constrained to one thread.
pub(crate) struct SdlPlatform {
    // Declaration order deliberately destroys the pump and window before their
    // subsystem handles and finally the process-level SDL context.
    event_pump: EventPump,
    window: Window,
    total_physical_memory_bytes: u64,
    text_input_active: bool,
    video: VideoSubsystem,
    _sdl: Sdl,
}

impl SdlPlatform {
    /// Initializes a hidden Vulkan-capable window before renderer construction.
    ///
    /// The window remains hidden until a Vulkan swapchain can present a fully
    /// initialized frame; this avoids exposing undefined startup contents.
    pub(crate) fn start(configuration: WindowConfiguration) -> Result<Self, PlatformError> {
        window_identity::establish()?;
        // SolCL keeps desktop composition active when another application gains
        // focus. Explicit user minimization remains available through the caption.
        let _ = sdl3::hint::set_video_minimize_on_focus_loss(false);
        let sdl = sdl3::init().map_err(|source| PlatformError::Initialize {
            message: source.to_string(),
        })?;
        let video = sdl.video().map_err(|source| PlatformError::Video {
            message: source.to_string(),
        })?;

        let (width, height, position) = match configuration.mode() {
            WindowMode::Windowed => (configuration.width(), configuration.height(), None),
            WindowMode::FullscreenWindowed => {
                let display = video.get_primary_display().map_err(|source| {
                    PlatformError::PrimaryDisplay {
                        message: source.to_string(),
                    }
                })?;
                let bounds =
                    display
                        .get_bounds()
                        .map_err(|source| PlatformError::PrimaryDisplay {
                            message: source.to_string(),
                        })?;
                let width = bounds.width();
                let height = bounds.height();
                if width == 0 || height == 0 {
                    return Err(PlatformError::PrimaryDisplay {
                        message: format!("invalid logical extent {width}x{height}"),
                    });
                }
                (width, height, Some((bounds.x(), bounds.y())))
            }
        };
        let mut builder = video.window(&crate::CLIENT_BUILD.to_string(), width, height);
        builder.vulkan().high_pixel_density().hidden();
        match configuration.mode() {
            WindowMode::Windowed => {
                builder.resizable();
            }
            WindowMode::FullscreenWindowed => {
                builder.borderless();
                if let Some((x, y)) = position {
                    builder.position(x, y);
                }
            }
        }
        let window = builder.build().map_err(|source| PlatformError::Window {
            message: source.to_string(),
        })?;
        window_identity::mark_primary_window(&window)?;
        let event_pump = sdl
            .event_pump()
            .map_err(|source| PlatformError::EventPump {
                message: source.to_string(),
            })?;
        let system_ram_mebibytes = sdl3::cpuinfo::system_ram();
        let total_physical_memory_bytes = u64::try_from(system_ram_mebibytes)
            .ok()
            .filter(|value| *value != 0)
            .and_then(|value| value.checked_mul(1_024 * 1_024))
            .ok_or(PlatformError::SystemRam)?;

        Ok(Self {
            event_pump,
            window,
            total_physical_memory_bytes,
            text_input_active: false,
            video,
            _sdl: sdl,
        })
    }

    /// Polls until it finds one admitted client event or exhausts SDL's queue.
    pub(crate) fn poll_event(&mut self) -> Option<super::TimedPlatformEvent> {
        static PROFILE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        let profile = *PROFILE.get_or_init(|| std::env::var_os("SOLARITY_FRAME_TIMINGS").is_some());
        loop {
            let started = profile.then(std::time::Instant::now);
            let started_cycles = profile
                .then(crate::platform::current_thread_cycles)
                .flatten();
            let event = self.event_pump.poll_event();
            if let Some(started) = started {
                let elapsed = started.elapsed();
                if elapsed >= std::time::Duration::from_millis(5) {
                    let cpu_cycles = started_cycles
                        .zip(crate::platform::current_thread_cycles())
                        .map(|(start, end)| end.saturating_sub(start));
                    tracing::info!(
                        elapsed_ms = elapsed.as_secs_f64() * 1_000.0,
                        ?cpu_cycles,
                        has_event = event.is_some(),
                        "profiled slow native SDL event poll"
                    );
                }
            }
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

    /// Returns whether desktop presentation must pause until the window is restored.
    ///
    /// Vulkan presentation against a minimized Win32 surface can repeatedly report
    /// an obsolete desktop-sized extent. Waiting for SDL's restore transition keeps
    /// swapchain recovery scoped to this window instead of churning compositor state.
    pub(crate) fn presentation_suspended(&self) -> bool {
        self.window.is_minimized()
    }

    /// Returns SDL's process-start total RAM report in bytes.
    pub(crate) const fn total_physical_memory_bytes(&self) -> u64 {
        self.total_physical_memory_bytes
    }

    /// Returns the platform-specific instance extensions required by SDL.
    pub(crate) fn vulkan_instance_extensions(&self) -> Result<Vec<String>, PlatformError> {
        self.window
            .vulkan_instance_extensions()
            .map_err(|source| PlatformError::VulkanExtensions {
                message: source.to_string(),
            })
    }

    /// Creates the native presentation surface owned by the supplied instance.
    ///
    /// # Safety
    ///
    /// `instance` must be live and must have enabled every extension returned by
    /// [`Self::vulkan_instance_extensions`]. The caller assumes surface ownership.
    pub(crate) unsafe fn create_vulkan_surface(
        &self,
        instance: ash::vk::Instance,
    ) -> Result<ash::vk::SurfaceKHR, PlatformError> {
        // SAFETY: The method contract binds the instance to SDL's exact required
        // extension set, and this owner keeps the native window alive.
        unsafe { self.window.vulkan_create_surface(instance) }.map_err(|source| {
            PlatformError::VulkanSurface {
                message: source.to_string(),
            }
        })
    }

    /// Reveals the window after rendering has presented initialized contents.
    pub(crate) fn show(&mut self) -> Result<(), PlatformError> {
        // SolCL waits for Win32/SDL to finish applying the visibility change.
        // Keeping the same boundary prevents the compositor from racing the
        // first visible client frame or a later caption-button restore.
        if self.window.show() && self.window.sync() {
            return Ok(());
        }
        Err(PlatformError::ShowWindow {
            message: sdl3::get_error().to_string(),
        })
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
}
