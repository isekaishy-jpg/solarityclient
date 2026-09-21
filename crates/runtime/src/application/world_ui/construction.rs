//! Main-thread FrameXML construction from prepared archives and current world facts.

use std::{ops::ControlFlow, task::Poll, time::Duration};

use solarity_asset::{ArchiveCatalog, AssetStoreHandle, BlpTextureCache, SpellNameCatalog};
use solarity_ecs::ActiveWorld;
use solarity_network::{WORLD_ACTION_BUTTON_COUNT, WorldActionButtons};
use solarity_ui::{
    AddonCatalog, FrameManager, FramePublication, FrameStartup, GlueError, UiActionBarState,
    UiEventPayload, UiScriptEnvironment, UiSpellBookTab, UiWorldState, UiZoneState,
};

use super::minimap::RuntimeMinimapScene;
use super::{
    RuntimeWorldUi, WorldUiSourceImage, mirror_timer, publish_realm_clock, requests_player_portrait,
};
use crate::application::ApplicationError;
use crate::application::character_directory::RuntimeCharacterMetadata;
use crate::application::login_ui::{RuntimeUiFrame, RuntimeUiResidency};
use crate::time::RealmClock;

/// Loading-only CPU allowance: preserve regular platform/presentation service
/// without adding hundreds of tiny, separately paced loading-card frames.
pub(in crate::application) const WORLD_UI_CONSTRUCTION_BUDGET: Duration = Duration::from_millis(8);

/// Owns the initial world image until ordered FrameXML construction completes.
/// Session publication is held by the caller while this task is pending; socket
/// workers retain their bounded queues, matching the former atomic load boundary.
pub(in crate::application) struct WorldUiConstruction {
    phase: Option<WorldUiConstructionPhase>,
    assets: AssetStoreHandle,
    archive_catalog: ArchiveCatalog,
    world: UiWorldState,
    zone: UiZoneState,
    action_bar: UiActionBarState,
    slots: [u32; WORLD_ACTION_BUTTON_COUNT],
    spell_names: SpellNameCatalog,
    corpse_marker: [f32; 2],
}

/// Entry callbacks and their publication follow the completed source execution.
enum WorldUiConstructionPhase {
    Scripts(FrameStartup),
    Entry(FramePublication<Vec<ApplicationError>>),
}

impl WorldUiConstruction {
    /// Captures the exact synchronous input image before any authored callback.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::application) fn begin(
        assets: AssetStoreHandle,
        archive_catalog: solarity_asset::ArchiveCatalog,
        logical_extent: (u32, u32),
        cvar_values: &[(String, String)],
        addon_catalog: &AddonCatalog,
        metadata: &RuntimeCharacterMetadata,
        active: &ActiveWorld,
        zone: UiZoneState,
        realm_clock: Option<&RealmClock>,
        action_buttons: Option<&WorldActionButtons>,
        player_ui: &crate::application::gameplay_coordinator::player_ui::RuntimePlayerUiState,
        general_tab_name: String,
        sound_output_names: Option<Vec<String>>,
        sources: WorldUiSourceImage,
    ) -> Result<Self, ApplicationError> {
        let environment = UiScriptEnvironment::new(logical_extent.0, logical_extent.1, false)
            .map_err(GlueError::from)?
            .with_client_clock(solarity_ui::UiClientClock::from_source(
                crate::platform::client_milliseconds,
            ));
        if let Some(names) = sound_output_names {
            environment.set_sound_output_devices(names);
        }
        let world = environment.world_state();
        world.tutorials().replace_flags(player_ui.tutorial_flags());
        world.set_release_timer(player_ui.release_timer());
        metadata.publish_active_player(active, &world)?;

        world.set_zone(zone.clone());
        publish_realm_clock(&world, realm_clock)?;
        // Wow.exe FUN_006D8750 owns a process-lifetime BSS array at
        // DAT_00AD9F6C, so FrameXML observes empty slots before the initial
        // server image rather than waiting for SMSG_ACTION_BUTTONS.
        let slots = action_buttons
            .and_then(WorldActionButtons::slots)
            .copied()
            .unwrap_or([0; WORLD_ACTION_BUTTON_COUNT]);
        let action_bar = environment.action_bar_state();
        action_bar.set_slots(slots);

        // The learned-spell packet and SkillLine join update this after entry.
        // A zero-spell General tab is stock's valid character-owned initial
        // image and prevents the pre-character empty spellbook state leaking
        // into synchronous FrameXML construction.
        environment
            .spell_book_state()
            .set_tabs(vec![UiSpellBookTab::new(
                general_tab_name,
                "Interface\\Icons\\INV_Misc_QuestionMark",
                0,
                0,
            )]);

        let spell_names = sources.spell_names;
        player_ui.resurrection().publish(&world, &spell_names);
        world.set_resurrection_offer(player_ui.offer());
        world.set_corpse_state(player_ui.corpse());
        for (index, notification) in player_ui.slots().iter().enumerate() {
            if let Some(notification) = notification {
                world.set_mirror_timer(
                    index,
                    mirror_timer::project_timer(&spell_names, *notification),
                );
            }
        }
        let startup = FrameManager::begin_with_sources(
            assets.clone(),
            environment,
            cvar_values,
            addon_catalog,
            sources.frame,
        );
        Ok(Self {
            phase: Some(WorldUiConstructionPhase::Scripts(startup)),
            assets,
            archive_catalog,
            world,
            zone,
            action_bar,
            slots,
            spell_names,
            corpse_marker: player_ui.corpse_marker(),
        })
    }

    /// Advances scripts, then ordered entry callbacks and one native publication.
    pub(in crate::application) fn advance(
        &mut self,
        budget: Duration,
    ) -> Poll<Result<(FrameManager, Vec<ApplicationError>), ApplicationError>> {
        let Some(phase) = self.phase.take() else {
            unreachable!("completed world UI construction must not be advanced again");
        };
        match phase {
            WorldUiConstructionPhase::Scripts(mut startup) => match startup.advance(budget) {
                Poll::Pending => {
                    self.phase = Some(WorldUiConstructionPhase::Scripts(startup));
                    Poll::Pending
                }
                Poll::Ready(Err(error)) => Poll::Ready(Err(error.into())),
                Poll::Ready(Ok(manager)) => {
                    self.phase = Some(WorldUiConstructionPhase::Entry(self.begin_entry(manager)));
                    Poll::Pending
                }
            },
            WorldUiConstructionPhase::Entry(mut entry) => match entry.advance(budget) {
                Poll::Pending => {
                    self.phase = Some(WorldUiConstructionPhase::Entry(entry));
                    Poll::Pending
                }
                Poll::Ready(result) => Poll::Ready(result.map_err(Into::into)),
            },
        }
    }

    /// Preserves 528010's entry order while yielding only between native events.
    fn begin_entry(&self, manager: FrameManager) -> FramePublication<Vec<ApplicationError>> {
        manager.minimap_state().set_corpse(self.corpse_marker);
        let world = self.world.clone();
        let mut events = [
            "VARIABLES_LOADED",
            "UPDATE_CHAT_WINDOWS",
            "PLAYER_LOGIN",
            "UPDATE_BINDINGS",
            "PLAYER_ENTERING_WORLD",
        ]
        .into_iter();
        let mut errors = Vec::new();
        manager.begin_suppressed_publication(move |manager| {
            if let Some(event) = events.next() {
                if let Err(error) = manager.dispatch_event(event, &UiEventPayload::empty()) {
                    errors.push(error.into());
                }
                return ControlFlow::Continue(());
            }
            if let Err(error) = mirror_timer::dispatch_world_entry_life(manager, &world) {
                errors.push(error);
            }
            ControlFlow::Break(std::mem::take(&mut errors))
        })
    }

    /// Admits renderer resources after the ordered world-entry publication.
    /// The caller must not consume intervening session notifications before this.
    pub(in crate::application) fn finish(
        self,
        renderer: &mut solarity_rendering::GpuPreparation<'_>,
        window_id: crate::WindowId,
        manager: FrameManager,
        startup_errors: Vec<ApplicationError>,
    ) -> Result<(RuntimeWorldUi, Vec<ApplicationError>), ApplicationError> {
        let Self {
            phase: _,
            assets,
            archive_catalog,
            world,
            zone,
            action_bar,
            slots,
            spell_names,
            corpse_marker: _,
        } = self;
        let mut texture_cache = BlpTextureCache::new();
        let mut texture_residency = RuntimeUiResidency::new(archive_catalog.clone());
        let frame = RuntimeUiFrame::prepare_frame(
            renderer,
            &manager,
            &mut texture_cache,
            &mut texture_residency,
        )?;
        let player_portrait_requested = requests_player_portrait(&manager);
        let minimap = RuntimeMinimapScene::new(&mut assets.borrow_mut(), archive_catalog)?;
        Ok((
            RuntimeWorldUi {
                manager,
                bindings: crate::InputBindingRouter::new(window_id),
                frame,
                minimap,
                presentation_revision: 0,
                texture_cache,
                texture_residency,
                portrait_mask: None,
                portrait_generation: None,
                player_portrait_requested,
                world,
                zone,
                action_bar,
                action_slots: slots,
                spell_names,
                chat_line_id: 0,
                dirty: false,
            },
            startup_errors,
        ))
    }
}

impl RuntimeWorldUi {
    /// Completes the same construction path synchronously for offline diagnostics.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::application) fn prepare(
        renderer: &mut solarity_rendering::GpuPreparation<'_>,
        window_id: crate::WindowId,
        assets: AssetStoreHandle,
        archive_catalog: solarity_asset::ArchiveCatalog,
        logical_extent: (u32, u32),
        cvar_values: &[(String, String)],
        addon_catalog: &AddonCatalog,
        metadata: &RuntimeCharacterMetadata,
        active: &ActiveWorld,
        zone: UiZoneState,
        realm_clock: Option<&RealmClock>,
        action_buttons: Option<&WorldActionButtons>,
        player_ui: &crate::application::gameplay_coordinator::player_ui::RuntimePlayerUiState,
        general_tab_name: String,
        sound_output_names: Option<Vec<String>>,
        sources: WorldUiSourceImage,
    ) -> Result<(Self, Vec<ApplicationError>), ApplicationError> {
        let mut construction = WorldUiConstruction::begin(
            assets,
            archive_catalog,
            logical_extent,
            cvar_values,
            addon_catalog,
            metadata,
            active,
            zone,
            realm_clock,
            action_buttons,
            player_ui,
            general_tab_name,
            sound_output_names,
            sources,
        )?;
        loop {
            if let Poll::Ready(result) = construction.advance(Duration::from_secs(60)) {
                let (manager, errors) = result?;
                return construction.finish(renderer, window_id, manager, errors);
            }
        }
    }
}
