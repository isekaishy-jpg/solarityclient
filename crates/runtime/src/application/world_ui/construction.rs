//! Main-thread FrameXML construction from prepared archives and current world facts.

use solarity_asset::{AssetStoreHandle, BlpTextureCache};
use solarity_ecs::ActiveWorld;
use solarity_network::{WORLD_ACTION_BUTTON_COUNT, WorldActionButtons};
use solarity_rendering::VulkanRenderer;
use solarity_ui::{
    AddonCatalog, FrameManager, GlueError, UiEventPayload, UiScriptEnvironment, UiSpellBookTab,
    UiZoneState,
};

use super::minimap::RuntimeMinimapScene;
use super::{
    RuntimeWorldUi, WorldUiSourceImage, mirror_timer, publish_realm_clock, requests_player_portrait,
};
use crate::application::ApplicationError;
use crate::application::character_directory::RuntimeCharacterMetadata;
use crate::application::login_ui::{RuntimeUiFrame, RuntimeUiResidency};
use crate::time::RealmClock;

impl RuntimeWorldUi {
    /// Constructs FrameXML behind an already-presented loading card and
    /// publishes stock's first-login event sequence before its first draw.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::application) fn prepare(
        renderer: &mut VulkanRenderer,
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
        let mut manager = FrameManager::start_with_sources(
            assets.clone(),
            environment,
            cvar_values,
            addon_catalog,
            sources.frame,
        )?;
        manager
            .minimap_state()
            .set_corpse(player_ui.corpse_marker());
        let mut startup_errors = Vec::new();
        manager.with_suppressed_sound_entries(|manager| {
            manager.with_deferred_presentation(|manager| {
                for event in [
                    "VARIABLES_LOADED",
                    // Chat settings are available before the world becomes visible.
                    // Stock applies colors and subscribes each tab on this event.
                    "UPDATE_CHAT_WINDOWS",
                    "PLAYER_LOGIN",
                    "UPDATE_BINDINGS",
                    "PLAYER_ENTERING_WORLD",
                ] {
                    if let Err(error) = manager.dispatch_event(event, &UiEventPayload::empty()) {
                        startup_errors.push(error.into());
                    }
                }
                if let Err(error) = mirror_timer::dispatch_world_entry_life(manager, &world) {
                    startup_errors.push(error);
                }
            })
        })?;
        let mut texture_cache = BlpTextureCache::new();
        let mut texture_residency = RuntimeUiResidency::new();
        let frame = RuntimeUiFrame::prepare_frame(
            renderer,
            &manager,
            &mut texture_cache,
            &mut texture_residency,
        )?;
        let player_portrait_requested = requests_player_portrait(&manager);
        let minimap = RuntimeMinimapScene::new(&mut assets.borrow_mut(), archive_catalog)?;
        Ok((
            Self {
                manager,
                bindings: crate::InputBindingRouter::new(window_id),
                frame,
                minimap,
                presentation_revision: 0,
                texture_cache,
                texture_residency,
                assets,
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
