//! Repeatable active-world transfer wiring on the presentation thread.

use solarity_asset::MapDifficultyCatalog;
use solarity_network::{WorldTransfer, WorldTransferTransport};

use super::ClientServices;
use crate::application::ApplicationError;
use crate::application::world_transfer::{
    RuntimeWorldReplacement, RuntimeWorldTransferEffect, RuntimeWorldTransferError,
};
use crate::loading::RuntimeLoadingScreen;

impl ClientServices {
    /// Drains packets in order, yielding immediately to synchronous verify-world
    /// replacement and deferring NEW_WORLD callbacks until the batch ends.
    pub(super) fn service_world_transfers(&mut self) -> Result<(), ApplicationError> {
        let path_distance_tolerance = self
            .world_ui
            .as_ref()
            .map_or_else(
                || self.glue.cvar_number("pathDistTol"),
                |ui| ui.cvar_number("pathDistTol"),
            )
            .unwrap_or(1.0);
        self.gameplay
            .set_path_distance_tolerance(path_distance_tolerance);
        if self.world_transfer.is_loading_map() {
            return Ok(());
        }
        if let Some(replacement) = self.world_transfer.take_deferred_replacement() {
            return self.begin_world_replacement(replacement);
        }
        loop {
            self.gameplay.service_with_game_objects(
                &mut |world, identity, notification, receipt_ms| {
                    self.game_objects
                        .observe_notification(
                            world,
                            identity,
                            notification,
                            receipt_ms,
                            &mut self.crt_rand,
                        )
                        .map_err(Into::into)
                },
            )?;
            let Some(transfer) = self.gameplay.take_world_transfer() else {
                break;
            };
            let Some(world) = self.gameplay.world() else {
                break;
            };
            let destination_exists = match transfer {
                WorldTransfer::NewWorld(location) | WorldTransfer::VerifyWorld(location) => {
                    self.terrain.contains_map(location.map_id())
                }
                WorldTransfer::Pending { .. } | WorldTransfer::Aborted { .. } => true,
            };
            match self
                .world_transfer
                .receive(transfer, world.map_id().value(), destination_exists)
            {
                RuntimeWorldTransferEffect::None => {}
                RuntimeWorldTransferEffect::OpenCard { map_id, transport } => {
                    self.open_world_transfer_card(map_id, transport)?;
                }
                RuntimeWorldTransferEffect::Abort {
                    map_id,
                    reason,
                    argument,
                } => {
                    let message_result =
                        self.present_world_transfer_abort(map_id, reason, argument);
                    self.loading_screen = None;
                    message_result?;
                    tracing::info!(map_id, reason, argument, "world transfer aborted");
                }
                RuntimeWorldTransferEffect::Replace(replacement) => {
                    return self.begin_world_replacement(replacement);
                }
                RuntimeWorldTransferEffect::InvalidMap(map_id) => {
                    tracing::warn!(
                        map_id,
                        "discarded world transfer with unknown Map.dbc identifier"
                    );
                }
            }
        }
        if let Some(replacement) = self.world_transfer.take_deferred_replacement() {
            self.begin_world_replacement(replacement)?;
        }
        Ok(())
    }

    /// Resolves MapDifficulty's exact optional message before the stock token arm.
    fn present_world_transfer_abort(
        &mut self,
        map_id: u32,
        reason: u8,
        argument: Option<u8>,
    ) -> Result<(), ApplicationError> {
        let (Some(map_name), Some(ui)) = (self.terrain.map_name(map_id), self.world_ui.as_mut())
        else {
            return Ok(());
        };
        let difficulty_message = if let (8, Some(difficulty)) = (reason, argument) {
            MapDifficultyCatalog::load(&mut self.assets.borrow_mut())?
                .message(map_id, u32::from(difficulty))
                .map(str::to_owned)
        } else {
            None
        };
        ui.transfer_aborted(map_name, reason, argument, difficulty_message.as_deref())
    }

    /// Opens the packet-owned card without changing any active-world resources.
    fn open_world_transfer_card(
        &mut self,
        map_id: u32,
        transport: Option<WorldTransferTransport>,
    ) -> Result<(), ApplicationError> {
        if let Some(transport) = transport
            .filter(|transport| transport.entry() != 0 && transport.source_map_id() != u32::MAX)
        {
            // 0x0040AD50 takes its dynamic arm only for a matching admitted
            // player transport. The ordinary destination card is stock's arm
            // when that object cannot be resolved or its template differs.
            let matches_player_transport = self.gameplay.world().is_some_and(|world| {
                world
                    .local_player_transport_guid()
                    .and_then(|guid| world.object_presentation(guid))
                    .is_some_and(|object| object.entry_id() == transport.entry())
            });
            if matches_player_transport {
                return Err(RuntimeWorldTransferError::DynamicTransportCard {
                    entry: transport.entry(),
                }
                .into());
            }
        }
        let extent = self.platform.pixel_extent();
        self.loading_screen = Some(
            self.loading_screen_cache
                .remove(&(map_id, extent))
                .map_or_else(
                    || {
                        RuntimeLoadingScreen::prepare(
                            &mut self.renderer,
                            &self.assets,
                            &mut self.ui_textures,
                            &self.loading_directory,
                            Some(map_id),
                            extent,
                        )
                    },
                    Ok,
                )?,
        );
        Ok(())
    }

    /// Retires the old replicated, visual, collision, and sound owners before
    /// the destination is published. FrameXML and the encrypted socket persist.
    fn begin_world_replacement(
        &mut self,
        replacement: RuntimeWorldReplacement,
    ) -> Result<(), ApplicationError> {
        let location = replacement.location();
        // This callback does not create a card. Pending-transfer or initial
        // login owns any existing card (0x00403B70); absent one, presentation
        // retains its last frame while the asynchronous map load completes.
        // A contained Lua fault must not leave the callback marked as loading
        // while the old map is still published, which could acknowledge the
        // wrong destination. Complete native ownership work before surfacing it.
        let leaving_event = self
            .world_ui
            .as_mut()
            .map(|ui| ui.leave_world())
            .transpose();
        self.input.clear_held();
        self.environment.disconnect();
        self.player.disconnect();
        self.game_objects.disconnect();
        self.terrain.disconnect();
        self.sound.disconnect()?;
        self.terrain_frame = None;
        self.gameplay.replace_world(location)?;
        self.area_triggers
            .enter_map(location.map_id(), crate::platform::client_milliseconds());
        leaving_event?;
        tracing::info!(
            map_id = location.map_id(),
            requires_acknowledgement = replacement.requires_acknowledgement(),
            "started destination world replacement"
        );
        Ok(())
    }

    /// Finishes map loading before asking the server for its replacement player.
    pub(super) fn complete_world_transfer_map(&mut self) -> Result<(), ApplicationError> {
        let Some(replacement) = self.world_transfer.loading() else {
            return Ok(());
        };
        if self.terrain_frame.is_none() {
            return Ok(());
        }
        if replacement.requires_acknowledgement() && !self.gameplay.acknowledge_world_transfer()? {
            return Ok(());
        }
        self.world_transfer.complete_map();
        tracing::info!(
            map_id = replacement.location().map_id(),
            acknowledgement_queued = replacement.requires_acknowledgement(),
            "completed destination map loading"
        );
        Ok(())
    }
}
