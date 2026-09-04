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
use super::login_ui::RuntimeUiFrame;
use crate::time::RealmClock;

/// A world UI could not be formed from authoritative entry state.
#[derive(Debug, Error)]
pub enum RuntimeWorldUiError {
    /// Packed server calendar state could not enter the FrameXML date type.
    #[error(transparent)]
    RealmDate(#[from] UiRealmDateError),
    /// Running server clock state could not enter the FrameXML time type.
    #[error(transparent)]
    RealmTime(#[from] UiRealmTimeError),
}

/// Retained FrameXML runtime paired with its current renderer generation.
pub(super) struct RuntimeWorldUi {
    manager: FrameManager,
    frame: RuntimeUiFrame,
    texture_cache: BlpTextureCache,
    world: solarity_ui::UiWorldState,
    zone: UiZoneState,
    action_bar: UiActionBarState,
    action_slots: [u32; 144],
    dirty: bool,
}

impl RuntimeWorldUi {
    /// Constructs FrameXML behind an already-presented loading card and
    /// publishes stock's first-login event sequence before its first draw.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare(
        renderer: &mut VulkanRenderer,
        assets: AssetStoreHandle,
        logical_extent: (u32, u32),
        cvar_values: &[(String, String)],
        addon_catalog: &AddonCatalog,
        metadata: &RuntimeCharacterMetadata,
        active: &ActiveWorld,
        zone: UiZoneState,
        realm_clock: Option<&RealmClock>,
        action_buttons: Option<&WorldActionButtons>,
        general_tab_name: String,
    ) -> Result<(Self, Vec<ApplicationError>), ApplicationError> {
        let environment = UiScriptEnvironment::new(logical_extent.0, logical_extent.1, false)
            .map_err(GlueError::from)?;
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
            FrameManager::start_shared(assets, environment, cvar_values, addon_catalog)?;
        let mut startup_errors = Vec::new();
        for event in [
            "VARIABLES_LOADED",
            "PLAYER_LOGIN",
            "UPDATE_BINDINGS",
            "PLAYER_ENTERING_WORLD",
        ] {
            if let Err(error) = manager.dispatch_event(event, &UiEventPayload::empty()) {
                startup_errors.push(error.into());
            }
        }
        let mut texture_cache = BlpTextureCache::new();
        let frame = RuntimeUiFrame::prepare_frame(renderer, &manager, &mut texture_cache)?;
        Ok((
            Self {
                manager,
                frame,
                texture_cache,
                world,
                zone,
                action_bar,
                action_slots: slots,
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
        let minimap_changed = zone.minimap_zone_text() != self.zone.minimap_zone_text();
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
        if minimap_changed {
            self.manager
                .dispatch_event("MINIMAP_ZONE_CHANGED", &UiEventPayload::empty())?;
        }
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
                &UiEventPayload::new([UiEventArgument::Integer((index + 1) as i64)])?,
            )?;
        }
        self.dirty = true;
        Ok(())
    }

    /// Returns one registered active-world console variable's current text.
    pub(super) fn cvar_value(&self, name: &str) -> Option<String> {
        self.manager.cvar_value(name)
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

    /// Rebuilds renderer resources after an event or update mutated live UI.
    pub(super) fn refresh(
        &mut self,
        renderer: &mut VulkanRenderer,
    ) -> Result<(), ApplicationError> {
        if self.dirty {
            self.frame
                .refresh_frame(renderer, &self.manager, &mut self.texture_cache)?;
            self.dirty = false;
        }
        Ok(())
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
        self.frame.draws()
    }
}

/// Publishes either the realm clock or the native BSS values visible before
/// SMSG_LOGIN_SETTIMESPEED. GetGameTime at 0x00608230 reads zero hour/minute;
/// CalendarGetDate at 0x005B8160 adds one to zero-based fields and 2000 to year.
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
