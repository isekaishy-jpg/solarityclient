//! Native logout callbacks delivered to the authored FrameXML camping dialogs.

use solarity_network::WorldLogout;
use solarity_ui::{UiEventArgument, UiEventPayload, UiLogoutState};

use super::RuntimeWorldUi;
use crate::application::ApplicationError;

impl RuntimeWorldUi {
    /// Shares synchronous Lua admission with ordered network dispatch.
    pub(in crate::application) fn logout_state(&self) -> UiLogoutState {
        self.world.logout()
    }

    /// 6B08B0 uses ERR_LOGOUT_FAILED; 6B0900 conditionally emits LOGOUT_CANCEL.
    pub(in crate::application) fn logout_update(
        &mut self,
        update: WorldLogout,
    ) -> Result<(), ApplicationError> {
        let state = self.world.logout();
        let event = match update {
            WorldLogout::Response { reason, instant } => state.response(reason, instant),
            WorldLogout::CancelAcknowledged => state.cancel_acknowledged(),
            WorldLogout::Complete => None,
        };
        let Some(event) = event else {
            return Ok(());
        };
        let payload = if event == "UI_ERROR_MESSAGE" {
            UiEventPayload::new([UiEventArgument::String(
                self.manager
                    .localized_text("ERR_LOGOUT_FAILED")
                    .map_err(solarity_ui::GlueError::from)?
                    .unwrap_or_default(),
            )])
        } else {
            UiEventPayload::empty()
        };
        self.dirty = true;
        let dispatched = self
            .manager
            .dispatch_event(event, &payload)
            .map(|_| ())
            .map_err(ApplicationError::from);
        // Both native callbacks clear pending after event handlers have run.
        if matches!(
            update,
            WorldLogout::Response { reason: 1.., .. } | WorldLogout::CancelAcknowledged
        ) {
            state.finish_cancellation();
        }
        dispatched
    }
}
