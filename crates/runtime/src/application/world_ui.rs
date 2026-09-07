//! Active-world FrameXML composition and renderer ownership.

use solarity_asset::{AssetStoreHandle, BlpTextureCache};
use solarity_ecs::ActiveWorld;
use solarity_network::{WORLD_ACTION_BUTTON_COUNT, WorldActionButtons};
use solarity_rendering::{UiPreparedDraw, VulkanRenderer};
use solarity_ui::{
    AddonCatalog, FrameManager, GlueError, UiActionBarState, UiEventArgument, UiEventPayload,
    UiKeyboardModifiers, UiPointerButton, UiPointerDispatch, UiProcessAction, UiRealmDate,
    UiRealmDateError, UiRealmTime, UiRealmTimeError, UiScriptEnvironment, UiSpellBookTab,
    UiZoneState,
};
use thiserror::Error;

use super::ApplicationError;
use super::character_directory::RuntimeCharacterMetadata;
use super::login_ui::{RuntimeUiFrame, RuntimeUiResidency};
use super::player_coordinator::{ResidentPlayerFrameInput, UnitPresentationGeneration};
use super::terrain_frame::TerrainFrame;
use crate::time::RealmClock;

mod minimap;
use minimap::RuntimeMinimapScene;

/// A world UI could not be formed from authoritative entry state.
#[derive(Debug, Error)]
pub enum RuntimeWorldUiError {
    /// The live minimap cannot form a finite world-to-UI projection.
    #[error(transparent)]
    Minimap(#[from] solarity_rendering::MinimapViewError),
    /// Packed server calendar state could not enter the FrameXML date type.
    #[error(transparent)]
    RealmDate(#[from] UiRealmDateError),
    /// Running server clock state could not enter the FrameXML time type.
    #[error(transparent)]
    RealmTime(#[from] UiRealmTimeError),
    /// A localized transfer message uses a format outside the recovered stock corpus.
    #[error("unsupported transfer message format {format}")]
    TransferMessageFormat {
        /// The authored format requiring an additional formatter capability.
        format: String,
    },
}

/// Retained FrameXML runtime paired with its current renderer generation.
pub(super) struct RuntimeWorldUi {
    manager: FrameManager,
    bindings: crate::InputBindingRouter,
    frame: RuntimeUiFrame,
    minimap: RuntimeMinimapScene,
    presentation_revision: u64,
    texture_cache: BlpTextureCache,
    texture_residency: RuntimeUiResidency,
    assets: AssetStoreHandle,
    portrait_mask: Option<solarity_rendering::BlpTextureHandle>,
    portrait_generation: Option<UnitPresentationGeneration>,
    player_portrait_requested: bool,
    world: solarity_ui::UiWorldState,
    zone: UiZoneState,
    action_bar: UiActionBarState,
    action_slots: [u32; 144],
    /// Stock ChatFrame.cpp's monotonic line identifier for admitted messages.
    chat_line_id: u32,
    dirty: bool,
}

impl RuntimeWorldUi {
    pub(super) fn set_modifier_keys(&self, keys: solarity_ui::UiModifierKeys) {
        self.manager.set_modifier_keys(keys);
    }
    pub(super) fn player_control_changed(&mut self, enabled: bool) -> Result<(), ApplicationError> {
        self.dirty = true;
        self.manager.dispatch_event(
            if enabled {
                "PLAYER_CONTROL_GAINED"
            } else {
                "PLAYER_CONTROL_LOST"
            },
            &UiEventPayload::empty(),
        )?;
        Ok(())
    }

    pub(super) fn set_input_event_time(&self, timestamp_ms: u32) {
        self.manager.set_input_event_time(timestamp_ms);
    }

    pub(super) fn take_movement_command(&self) -> Option<solarity_ui::UiMovementCommand> {
        self.manager.take_movement_command()
    }

    /// Preserves FrameXML sound calls in the same order as GlueXML calls.
    pub(super) fn take_media_action(&self) -> Option<solarity_ui::UiGlueMediaAction> {
        self.manager.take_media_action()
    }

    /// Publishes the process sound owner's resolved device after a restart.
    pub(super) fn publish_sound_output(
        &self,
        names: Vec<String>,
        index: usize,
        name: &str,
    ) -> Result<(), solarity_ui::UiScriptError> {
        self.manager.set_sound_output_devices(names);
        self.manager.set_sound_output_selection(index, name)
    }

    /// Delivers the transfer handler's localized system chat event before card dismissal.
    pub(super) fn transfer_aborted(
        &mut self,
        map_name: &str,
        reason: u8,
        argument: Option<u8>,
        difficulty_message: Option<&str>,
    ) -> Result<(), ApplicationError> {
        let format = if let Some(message) = difficulty_message.filter(|message| !message.is_empty())
        {
            Some(message.to_owned())
        } else if let Some(token) = transfer_abort_token(reason, argument) {
            self.manager
                .localized_text(&token)
                .map_err(GlueError::from)?
        } else {
            None
        };
        let Some(format) = format else {
            return Ok(());
        };
        let message = format_transfer_message(&format, map_name)?;
        self.chat_line_id = self.chat_line_id.wrapping_add(1);
        // 0x00403910 calls 0x00509DD0 with chat type zero and all other
        // parameters zero. 0x004FDBC0 emits thirteen values; Languages.dbc
        // has no ID-zero row, so its language name is the empty string.
        self.dirty = true;
        self.manager.dispatch_event(
            "CHAT_MSG_SYSTEM",
            &UiEventPayload::new([
                UiEventArgument::String(message),
                UiEventArgument::String(String::new()),
                UiEventArgument::String(String::new()),
                UiEventArgument::String(String::new()),
                UiEventArgument::String(String::new()),
                UiEventArgument::String(String::new()),
                UiEventArgument::Integer(0),
                UiEventArgument::Integer(0),
                UiEventArgument::String(String::new()),
                UiEventArgument::Integer(0),
                UiEventArgument::Integer(i64::from(self.chat_line_id)),
                UiEventArgument::String(String::new()),
                UiEventArgument::Integer(0),
            ]),
        )?;
        Ok(())
    }

    /// Delivers the live-world exit event without reconstructing FrameXML.
    pub(super) fn leave_world(&mut self) -> Result<(), ApplicationError> {
        self.dirty = true;
        let release = self.route_binding(
            &crate::PlatformEvent::ApplicationDidEnterBackground,
            crate::KeyModifiers::NONE,
            true,
        );
        let leave = self
            .manager
            .dispatch_event("PLAYER_LEAVING_WORLD", &UiEventPayload::empty())
            .map(|_| ())
            .map_err(ApplicationError::from);
        release.and(leave)
    }

    /// Refreshes the new replicated player before the repeatable entry event.
    pub(super) fn enter_replacement_world(
        &mut self,
        metadata: &RuntimeCharacterMetadata,
        active: &ActiveWorld,
    ) -> Result<(), ApplicationError> {
        metadata.publish_active_player(active, &self.world)?;
        self.dirty = true;
        self.manager
            .dispatch_event("PLAYER_ENTERING_WORLD", &UiEventPayload::empty())?;
        Ok(())
    }

    /// Constructs FrameXML behind an already-presented loading card and
    /// publishes stock's first-login event sequence before its first draw.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare(
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
        general_tab_name: String,
        sound_output_names: Option<Vec<String>>,
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

        let mut manager =
            FrameManager::start_shared(assets.clone(), environment, cvar_values, addon_catalog)?;
        let mut startup_errors = Vec::new();
        manager.with_suppressed_sound_entries(|manager| {
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
        });
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
                chat_line_id: 0,
                dirty: false,
            },
            startup_errors,
        ))
    }

    /// Replaces stock's zero-initialized time globals once the realm packet is
    /// authoritative. Visible GameTime OnUpdate handlers observe the shared
    /// values without reconstructing FrameXML.
    pub(super) fn synchronize_realm_clock(
        &mut self,
        realm_clock: &RealmClock,
    ) -> Result<(), RuntimeWorldUiError> {
        publish_realm_clock(&self.world, Some(realm_clock))
    }

    /// Publishes a changed area projection and emits the stock events consumed
    /// by the zone banner, minimap, and zone-text owners.
    pub(super) fn synchronize_zone(&mut self, zone: UiZoneState) -> Result<(), ApplicationError> {
        if zone == self.zone {
            return Ok(());
        }
        let top_level_changed = zone.real_zone_text() != self.zone.real_zone_text();
        let sub_zone_changed = zone.sub_zone_text() != self.zone.sub_zone_text();
        self.world.set_zone(zone.clone());
        self.zone = zone;
        if top_level_changed {
            self.manager
                .dispatch_event("ZONE_CHANGED_NEW_AREA", &UiEventPayload::empty())?;
        }
        if sub_zone_changed {
            self.manager
                .dispatch_event("ZONE_CHANGED", &UiEventPayload::empty())?;
        }
        // Build 12340's Minimap.xml subscribes to the same three ZONE_CHANGED
        // events as the zone banner. There is no MINIMAP_ZONE_CHANGED native
        // event in the executable registry; the minimap label is already
        // refreshed by the top-level or sub-zone branch above.
        self.dirty = true;
        Ok(())
    }

    /// Applies a replacement server action-bar image and emits one-based slot
    /// events for exactly the values that changed.
    pub(super) fn synchronize_action_buttons(
        &mut self,
        buttons: &WorldActionButtons,
    ) -> Result<(), ApplicationError> {
        let Some(slots) = buttons.slots() else {
            return Ok(());
        };
        if slots == &self.action_slots {
            return Ok(());
        }
        let previous = self.action_slots;
        self.action_bar.set_slots(*slots);
        self.action_slots = *slots;
        for (index, (before, after)) in previous.iter().zip(slots).enumerate() {
            if before == after {
                continue;
            }
            self.manager.dispatch_event(
                "ACTIONBAR_SLOT_CHANGED",
                &UiEventPayload::new([UiEventArgument::Integer((index + 1) as i64)]),
            )?;
        }
        self.dirty = true;
        Ok(())
    }

    /// Returns one registered active-world console variable's current text.
    pub(super) fn cvar_value(&self, name: &str) -> Option<String> {
        self.manager.cvar_value(name)
    }

    pub(super) fn cvar_number(&self, name: &str) -> Option<f32> {
        self.manager.cvar_number(name)
    }

    pub(super) fn cvar_revision(&self) -> u64 {
        self.manager.cvar_revision()
    }

    /// Takes profile-backed CVars changed by built-in FrameXML Lua.
    pub(super) fn take_changed_cvars(&self) -> Vec<(String, String)> {
        self.manager.take_changed_cvars()
    }

    /// Advances visible handlers and records whether GPU presentation changed.
    pub(super) fn update(&mut self, elapsed_seconds: f64) -> Result<(), ApplicationError> {
        self.dirty |= self.manager.update(elapsed_seconds)?;
        Ok(())
    }

    /// Takes one authored FrameXML update fault contained by the UI runtime.
    pub(super) fn take_update_failure(&mut self) -> Option<String> {
        self.manager.take_update_failure()
    }

    /// Updates a requested portrait only when the resident appearance changes.
    pub(super) fn synchronize_portrait(
        &mut self,
        renderer: &mut VulkanRenderer,
        terrain: &TerrainFrame,
        player: &ResidentPlayerFrameInput<'_>,
    ) -> Result<(), ApplicationError> {
        if self.dirty {
            self.player_portrait_requested = requests_player_portrait(&self.manager);
        }
        if !self.player_portrait_requested
            || self
                .portrait_generation
                .as_ref()
                .is_some_and(|generation| generation.matches(player.generation()))
        {
            return Ok(());
        }
        let mask = if let Some(mask) = self.portrait_mask {
            mask
        } else {
            let path = solarity_asset::AssetPath::new(
                "Interface\\CharacterFrame\\TempPortraitAlphaMask.blp",
            )?;
            let source =
                solarity_asset::BlpTextureSource::load(&mut self.assets.borrow_mut(), &path)?;
            let mask =
                renderer.upload_blp_texture(&source, solarity_rendering::BlpColorSpace::Linear)?;
            self.portrait_mask = Some(mask);
            mask
        };
        if terrain.render_player_portrait(renderer, player, mask)? {
            self.portrait_generation = Some(player.generation().clone());
            self.dirty = true;
        }
        Ok(())
    }

    /// Rebuilds renderer resources after an event or update mutated live UI.
    pub(super) fn refresh(
        &mut self,
        renderer: &mut VulkanRenderer,
    ) -> Result<(), ApplicationError> {
        if self.dirty {
            self.frame.refresh_frame(
                renderer,
                &self.manager,
                &mut self.texture_cache,
                &mut self.texture_residency,
            )?;
            self.dirty = false;
            self.presentation_revision = self.presentation_revision.wrapping_add(1);
        }
        Ok(())
    }

    pub(super) fn synchronize_minimap(
        &mut self,
        renderer: &mut VulkanRenderer,
        cpu: &solarity_cpu::CpuExecutor,
        map: Option<&solarity_asset::TerrainMap>,
        player: Option<solarity_ecs::WorldTransform>,
    ) -> Result<(), ApplicationError> {
        self.minimap.synchronize(
            renderer,
            cpu,
            &self.manager,
            &self.frame,
            self.presentation_revision,
            map,
            player,
        )
    }

    pub(super) fn minimap_ready(&self) -> bool {
        self.minimap.ready()
    }

    /// Executes unclaimed bindings and preserves release delivery across UI capture.
    pub(super) fn route_binding(
        &mut self,
        event: &crate::PlatformEvent,
        modifiers: crate::KeyModifiers,
        captured: bool,
    ) -> Result<(), ApplicationError> {
        // An authored error may still leave valid presentation mutations.
        // Refresh the renderer after every dispatched batch, including errors.
        match self
            .bindings
            .route_to_frame(event, modifiers, captured, &mut self.manager)
        {
            Ok(count) => {
                self.dirty |= count != 0;
                Ok(())
            }
            Err(error) => {
                self.dirty = true;
                Err(error.into())
            }
        }
    }

    /// Routes a pointer button and marks an addressed frame generation dirty.
    pub(super) fn pointer_button(
        &mut self,
        position: (f64, f64),
        button: UiPointerButton,
        pressed: bool,
        click_count: u8,
        modifiers: UiKeyboardModifiers,
    ) -> Result<UiPointerDispatch, ApplicationError> {
        let dispatch = self.manager.pointer_button_with_modifiers(
            position,
            button,
            pressed,
            click_count,
            modifiers,
        )?;
        self.dirty |= dispatch.object_index().is_some();
        Ok(dispatch)
    }

    /// Updates pointer focus and a captured slider.
    pub(super) fn pointer_motion(
        &mut self,
        position: (f64, f64),
    ) -> Result<Option<usize>, ApplicationError> {
        let target = self.manager.pointer_motion(position)?;
        self.dirty |= target.is_some();
        Ok(target)
    }

    /// Routes one wheel delta to the frontmost eligible object.
    pub(super) fn pointer_wheel(
        &mut self,
        position: (f64, f64),
        delta: f64,
    ) -> Result<Option<usize>, ApplicationError> {
        let target = self.manager.pointer_wheel(position, delta)?;
        self.dirty |= target.is_some();
        Ok(target)
    }

    /// Routes one keyboard transition to FrameXML.
    pub(super) fn keyboard_key(
        &mut self,
        key: &str,
        pressed: bool,
        modifiers: UiKeyboardModifiers,
    ) -> Result<Option<usize>, ApplicationError> {
        let target = self.manager.keyboard_key(key, pressed, modifiers)?;
        self.dirty |= target.is_some();
        Ok(target)
    }

    /// Routes committed text to the focused world edit box.
    pub(super) fn text_input(&mut self, text: &str) -> Result<Option<usize>, ApplicationError> {
        let target = self.manager.text_input(text)?;
        self.dirty |= target.is_some();
        Ok(target)
    }

    /// Routes an input-method composition to the focused world edit box.
    pub(super) fn text_composition(
        &mut self,
        text: &str,
    ) -> Result<Option<usize>, ApplicationError> {
        let target = self.manager.text_composition(text)?;
        self.dirty |= target.is_some();
        Ok(target)
    }

    /// Returns whether FrameXML currently owns a visible edit box.
    pub(super) fn has_focused_edit_box(&self) -> bool {
        self.manager.focused_edit_box().is_some()
    }

    /// Takes the oldest process action emitted by active-world Lua.
    pub(super) fn take_process_action(&self) -> Option<UiProcessAction> {
        self.manager.take_process_action()
    }

    /// Returns the logical UI canvas used by the world compositor.
    pub(super) const fn logical_extent(&self) -> [f32; 2] {
        self.frame.logical_extent()
    }

    /// Returns renderer-validated FrameXML draws in stock order.
    pub(super) fn draws(&self) -> &[UiPreparedDraw] {
        self.minimap.draws(&self.frame)
    }
}

/// Selects the exact localization branch at 0x00403910. Reason eight's DBC
/// message is resolved first by the caller, before its numbered-token arm.
fn transfer_abort_token(reason: u8, argument: Option<u8>) -> Option<String> {
    Some(match reason {
        1 => "TRANSFER_ABORT_ERROR".to_owned(),
        2 => "TRANSFER_ABORT_MAX_PLAYERS".to_owned(),
        3 | 12..=14 => "TRANSFER_ABORT_NOT_FOUND".to_owned(),
        4 => "TRANSFER_ABORT_TOO_MANY_INSTANCES".to_owned(),
        6 => "TRANSFER_ABORT_ZONE_IN_COMBAT".to_owned(),
        7 => format!("TRANSFER_ABORT_INSUF_EXPAN_LVL{}", argument?),
        8 => format!("TRANSFER_ABORT_DIFFICULTY{}", u32::from(argument?) + 1),
        9 => format!("TRANSFER_ABORT_UNIQUE_MESSAGE{}", argument?),
        10 => "TRANSFER_ABORT_TOO_MANY_REALM_INSTANCES".to_owned(),
        11 => "TRANSFER_ABORT_NEED_GROUP".to_owned(),
        15 => "TRANSFER_ABORT_REALM_ONLY".to_owned(),
        16 => "TRANSFER_ABORT_MAP_NOT_ALLOWED".to_owned(),
        _ => return None,
    })
}

/// Formats the single map-name argument used by stock transfer messages.
/// The pinned enUS token corpus uses plain text or one `%s`; `%%` retains
/// printf's literal-percent semantics without evaluating any Lua/AddOn code.
fn format_transfer_message(format: &str, map_name: &str) -> Result<String, RuntimeWorldUiError> {
    let mut text = String::with_capacity(format.len() + map_name.len());
    let mut characters = format.chars();
    let mut map_substituted = false;
    while let Some(character) = characters.next() {
        if character != '%' {
            text.push(character);
            continue;
        }
        match characters.next() {
            Some('%') => text.push('%'),
            Some('s') if !map_substituted => {
                text.push_str(map_name);
                map_substituted = true;
            }
            _ => {
                return Err(RuntimeWorldUiError::TransferMessageFormat {
                    format: format.to_owned(),
                });
            }
        }
    }
    Ok(text)
}

/// Publishes either the realm clock or the native BSS values visible before
/// SMSG_LOGIN_SETTIMESPEED. GetGameTime at 0x00608230 reads zero hour/minute;
/// CalendarGetDate at 0x005B8160 adds one to zero-based fields and 2000 to year.
fn requests_player_portrait(manager: &FrameManager) -> bool {
    manager.render_plan().mesh().batches().iter().any(|batch| {
        matches!(batch.source(), solarity_rendering::UiRenderSource::UnitPortrait(unit) if unit == "player")
    })
}

fn publish_realm_clock(
    world: &solarity_ui::UiWorldState,
    realm_clock: Option<&RealmClock>,
) -> Result<(), RuntimeWorldUiError> {
    let (date, time) = if let Some(realm_clock) = realm_clock {
        let source = realm_clock.source();
        let half_minutes = realm_clock.half_minutes();
        (
            UiRealmDate::new(
                source.weekday_index() + 1,
                source.month_index() + 1,
                source.month_day(),
                source.year(),
            )?,
            UiRealmTime::new((half_minutes / 120) as u8, ((half_minutes % 120) / 2) as u8)?,
        )
    } else {
        (UiRealmDate::new(1, 1, 1, 2_000)?, UiRealmTime::new(0, 0)?)
    };
    world.set_realm_date(date);
    world.set_realm_time(time);
    Ok(())
}
