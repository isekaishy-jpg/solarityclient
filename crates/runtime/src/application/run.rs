//! Main-thread lifetime and termination decisions for the client process.

use crate::platform::{PlatformEvent, WindowEvent};

/// Stock-relevant reason the persistent client loop stopped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApplicationExitReason {
    /// SDL received the process-wide quit request.
    QuitRequested,
    /// The host platform announced immediate application termination.
    ApplicationTerminating,
    /// The operating system requested closure of the primary client window.
    PrimaryWindowCloseRequested,
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
