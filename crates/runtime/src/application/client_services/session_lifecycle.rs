//! World retirement and stock Glue notification after a realm connection is lost.

use solarity_ui::{UiEventArgument, UiEventPayload, UiGlueNetworkStatus};

use super::ClientServices;
use crate::application::ApplicationError;
use crate::application::world_coordinator::RuntimeCharacterScreenRequests;

impl ClientServices {
    /// 6B2180/406510 retire the character while retaining the authenticated realm.
    pub(super) fn complete_logout(
        &mut self,
        session: solarity_network::WorldSession<tokio::net::TcpStream>,
    ) -> Result<(), ApplicationError> {
        // 6B2180 calls 6B1840 before retiring the player. Its pending cancellation
        // closes the camping popup while the world UI still exists.
        let cancelled = self
            .world_ui
            .as_mut()
            .map(|ui| ui.logout_update(solarity_network::WorldLogout::CancelAcknowledged))
            .transpose();
        let quit = self
            .world_ui
            .as_ref()
            .is_some_and(|ui| ui.logout_state().quits_process());
        let retired = self.retire_world();
        self.character_screen_published = false;
        self.character_directory_published = false;
        self.pending_character_screen_requests = RuntimeCharacterScreenRequests::default();
        if quit {
            self.quit_after_logout = true;
        } else if let Err(error) = self.world.resume_character_screen(session) {
            return self.publish_world_failure(error);
        }
        self.glue_ui_dirty = true;
        cancelled.map(|_| ()).and(retired)
    }

    /// Releases character-owned state before another world or Glue screen can publish.
    pub(super) fn retire_world(&mut self) -> Result<(), ApplicationError> {
        let leaving = self
            .world_ui
            .as_mut()
            .map(|ui| ui.retire_world())
            .transpose();
        let profile = self.persist_active_cvars();
        let camera = self.save_character_camera();
        self.character_profile = None;
        self.input.clear_held();
        self.player_movement.reset();
        self.gameplay.disconnect();
        self.area_triggers.disconnect();
        self.world_transfer.disconnect();
        self.environment.disconnect();
        self.player.disconnect();
        self.game_objects.disconnect();
        self.terrain.disconnect();
        let sound = self.sound.disconnect().map_err(ApplicationError::from);
        let frame = self
            .terrain_frame
            .take()
            .map(|frame| frame.retire(&mut self.renderer))
            .transpose()
            .map_err(ApplicationError::from);
        self.world_ui = None;
        self.loading_screen = None;
        leaving
            .map(|_| ())
            .and(profile)
            .and(camera)
            .and(sound)
            .and(frame.map(|_| ()))
    }

    /// Ends both realm phases while retaining the process-owned Glue environment.
    pub(super) fn disconnect_from_server(&mut self) -> Result<(), ApplicationError> {
        let retired = self.retire_world();
        self.login.disconnect();
        self.world.disconnect();
        self.loading_screen_cache.clear();
        self.loading_screen_prewarm_queue.clear();
        self.authentication_prewarm_active = false;
        self.realm_directory_published = false;
        self.character_screen_published = false;
        self.character_directory_published = false;
        self.pending_character_screen_requests = RuntimeCharacterScreenRequests::default();
        self.pending_realm_id = None;
        self.selected_realm = None;
        self.glue
            .set_realm_directory(self.realm_metadata.empty_directory());
        self.glue.set_network_status(UiGlueNetworkStatus::default());
        self.glue_ui_dirty = true;
        retired
    }

    /// 0x004DA9D0 retires the world before GlueParent presents its disconnect dialog.
    pub(super) fn connection_lost(&mut self) -> Result<(), ApplicationError> {
        let retired = self.disconnect_from_server();
        let notified = self
            .glue
            .dispatch_event(
                "DISCONNECTED_FROM_SERVER",
                &UiEventPayload::new([UiEventArgument::Integer(0)]),
            )
            .map(|_| ())
            .map_err(ApplicationError::from);
        retired.and(notified)
    }
}
