//! Diagnostic input is consumed before stock bindings, including repeat and release.

use solarity_profiling::Capture;

use crate::input::stock_keyboard_name;
use crate::platform::{ButtonState, PlatformEvent, WindowId};
use crate::{CLIENT_BUILD, RuntimeConfiguration};

#[cfg(test)]
#[path = "../../../tests/application/f10_instrumentation.rs"]
mod tests;

/// Application-owned capture; background report work is joined at shutdown.
pub(in crate::application) struct RuntimeInstrumentation {
    capture: Capture,
}

impl RuntimeInstrumentation {
    /// Captures only explicit diagnostic configuration, never login credentials.
    pub(in crate::application) fn new(configuration: &RuntimeConfiguration) -> Self {
        let identity = format!(
            "product={CLIENT_BUILD}\nrevision={}\ndirty={}\ncpu_workers={}\nnetwork_workers={}",
            CLIENT_BUILD.revision(),
            CLIENT_BUILD.is_dirty(),
            configuration.cpu_pool().worker_count(),
            configuration.network_workers()
        );
        let mut profiler = Self {
            capture: Capture::new(configuration.profile_root(), identity),
        };
        if std::env::var_os("SOLARITY_FRAME_TIMINGS").is_some()
            || std::env::var_os("SOLARITY_GPU_TIMINGS").is_some()
            || std::env::var_os("SOLARITY_UI_TIMINGS").is_some()
        {
            profiler.toggle();
        }
        profiler
    }

    /// F10 is a deliberate diagnostic shortcut rather than a stock gameplay binding.
    pub(in crate::application) fn service_event(
        &mut self,
        event: &PlatformEvent,
        window: WindowId,
    ) -> bool {
        let PlatformEvent::Key(key) = event else {
            return false;
        };
        if key.window_id != window || key.scan_code.and_then(stock_keyboard_name) != Some("F10") {
            return false;
        }
        if key.state == ButtonState::Pressed && !key.is_repeat {
            self.toggle();
        }
        true
    }

    /// Reports one transition; no per-frame or per-model log formatting is required.
    fn toggle(&mut self) {
        match self.capture.toggle() {
            Ok((active, path)) => {
                tracing::info!(active, path = %path.display(), "changed F10 instrumentation capture")
            }
            Err(error) => tracing::warn!(%error, "could not toggle F10 instrumentation capture"),
        }
    }

    /// A finishing report cannot block the presentation thread.
    pub(in crate::application) fn poll(&mut self) {
        if let Some(result) = self.capture.poll() {
            match result {
                Ok(path) => {
                    tracing::info!(path = %path.display(), "saved F10 instrumentation capture")
                }
                Err(error) => tracing::warn!(%error, "could not save F10 instrumentation capture"),
            }
        }
    }

    /// Shutdown owns the final join and reports diagnostic failures separately.
    pub(in crate::application) fn shutdown(&mut self) {
        if let Err(error) = self.capture.shutdown() {
            tracing::warn!(%error, "could not finish F10 instrumentation capture");
        }
    }
}
