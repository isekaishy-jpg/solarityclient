//! Main-thread lifetime and termination decisions for the client process.

use crate::platform::{MouseMotionEvent, PlatformEvent, WindowEvent};

/// Maximum translated SDL events admitted before presentation gets a turn.
///
/// Raw state is retained as each event is polled, while expensive UI routing
/// happens below this boundary. A finite slice prevents an input producer from
/// indefinitely starving animation and swapchain presentation.
pub(super) const MAX_PLATFORM_EVENTS_PER_FRAME: usize = 256;

/// Retains the last absolute cursor position for one consecutive motion run.
///
/// Relative deltas are preserved for any event consumer added after this
/// boundary. Motion is never merged across windows or across a non-motion
/// event, so pointer-button and focus ordering remains identical to SDL order.
pub(super) fn coalesce_mouse_motion(
    pending: &mut Option<(u32, MouseMotionEvent)>,
    timestamp_ms: u32,
    next: MouseMotionEvent,
) -> Option<(u32, MouseMotionEvent)> {
    let Some((current_time, current)) = pending.as_mut() else {
        *pending = Some((timestamp_ms, next));
        return None;
    };
    if current.window_id != next.window_id {
        return pending.replace((timestamp_ms, next));
    }
    current.x = next.x;
    current.y = next.y;
    current.delta_x += next.delta_x;
    current.delta_y += next.delta_y;
    *current_time = timestamp_ms;
    None
}

/// Stock-relevant reason the persistent client loop stopped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApplicationExitReason {
    /// SDL received the process-wide quit request.
    QuitRequested,
    /// The host platform announced immediate application termination.
    ApplicationTerminating,
    /// The operating system requested closure of the primary client window.
    PrimaryWindowCloseRequested,
    /// Built-in GlueXML or FrameXML requested orderly client termination.
    UiQuitRequested,
}

/// Immutable facts collected while the main-thread event loop was active.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApplicationRunReport {
    exit_reason: ApplicationExitReason,
    admitted_event_count: u64,
}

impl ApplicationRunReport {
    /// Constructs the final report after the termination event is admitted.
    pub(super) const fn new(exit_reason: ApplicationExitReason, admitted_event_count: u64) -> Self {
        Self {
            exit_reason,
            admitted_event_count,
        }
    }

    /// Returns the event that ended the persistent client lifetime.
    #[must_use]
    pub const fn exit_reason(self) -> ApplicationExitReason {
        self.exit_reason
    }

    /// Returns all translated events admitted during this invocation.
    #[must_use]
    pub const fn admitted_event_count(self) -> u64 {
        self.admitted_event_count
    }
}

/// Selects only process or primary-window events that end stock client life.
pub(super) fn exit_reason(
    event: &PlatformEvent,
    primary_window: u32,
) -> Option<ApplicationExitReason> {
    match event {
        PlatformEvent::QuitRequested => Some(ApplicationExitReason::QuitRequested),
        PlatformEvent::ApplicationTerminating => {
            Some(ApplicationExitReason::ApplicationTerminating)
        }
        PlatformEvent::Window {
            window_id,
            event: WindowEvent::CloseRequested,
        } if window_id.value() == primary_window => {
            Some(ApplicationExitReason::PrimaryWindowCloseRequested)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::coalesce_mouse_motion;
    use crate::platform::{MouseMotionEvent, WindowId};

    #[test]
    fn consecutive_motion_retains_last_position_and_total_delta() {
        let mut pending = None;
        assert_eq!(
            coalesce_mouse_motion(&mut pending, u32::MAX, motion(7, 10.0, 20.0, 2.0, -1.0)),
            None
        );
        assert_eq!(
            coalesce_mouse_motion(&mut pending, 2, motion(7, 14.0, 25.0, 4.0, 5.0)),
            None
        );

        assert_eq!(pending, Some((2, motion(7, 14.0, 25.0, 6.0, 4.0))));
    }

    #[test]
    fn motion_from_another_window_flushes_before_replacement() {
        let mut pending = Some((93, motion(7, 10.0, 20.0, 2.0, -1.0)));
        let next = motion(8, 30.0, 40.0, 5.0, 6.0);

        assert_eq!(
            coalesce_mouse_motion(&mut pending, 99, next),
            Some((93, motion(7, 10.0, 20.0, 2.0, -1.0)))
        );
        assert_eq!(pending, Some((99, next)));
    }

    const fn motion(
        window_id: u32,
        x: f32,
        y: f32,
        delta_x: f32,
        delta_y: f32,
    ) -> MouseMotionEvent {
        MouseMotionEvent {
            window_id: WindowId::new(window_id),
            x,
            y,
            delta_x,
            delta_y,
        }
    }
}
