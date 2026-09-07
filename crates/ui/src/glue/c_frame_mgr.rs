//! Persistent ownership of the built-in active-world interface.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use solarity_asset::{AssetStoreHandle, BlpTextureCache};

use super::{GlueError, GlueManager};
use crate::{
    AddonCatalog, UiBindingAssignments, UiBindingCatalog, UiEventDispatch, UiEventError,
    UiEventPayload, UiGlyphAtlasPlan, UiKeyboardModifiers, UiPointerButton, UiPointerDispatch,
    UiProcessAction, UiRenderPlan, UiScriptEnvironment, UiTextureAssetBindings,
};

/// Complete built-in FrameXML state retained across the active-world lifetime.
///
/// FrameXML and GlueXML share the stock XML/Lua object engine, but their
/// manifests, event registries, and process lifetimes remain distinct. This
/// facade deliberately exposes only the operations owned by an active world.
pub struct FrameManager {
    owner: GlueManager,
    binding_catalog: UiBindingCatalog,
    binding_assignments: Rc<RefCell<UiBindingAssignments>>,
    binding_functions: HashMap<String, mlua::Function>,
    movement_input: crate::script::UiMovementInput,
}

impl FrameManager {
    /// Publishes the physical modifier snapshot before native Lua queries.
    pub fn set_modifier_keys(&self, keys: crate::UiModifierKeys) {
        self.owner.set_modifier_keys(keys);
    }

    /// Executes a binding with its current pointer-button context.
    ///
    /// # Errors
    /// Returns the same command and presentation failures as [`Self::invoke_binding`].
    pub fn invoke_binding_with_mouse_button(
        &mut self,
        name: &str,
        pressed: bool,
        button: Option<UiPointerButton>,
    ) -> Result<bool, UiEventError> {
        let _button_context = self.owner.mouse_button_scope(button);
        self.invoke_binding(name, pressed)
    }
    /// Resolves a native message token, preserving missing or empty stock text.
    ///
    /// # Errors
    ///
    /// Returns [`crate::UiScriptError`] if Lua cannot read the localization global.
    pub fn localized_text(&self, token: &str) -> Result<Option<String>, crate::UiScriptError> {
        self.owner
            .bundle()
            .lua()
            .globals()
            .raw_get::<Option<String>>(token)
            .map(|text| text.filter(|text| !text.is_empty()))
            .map_err(|error| crate::UiScriptError::Execution {
                label: format!("localization token {token}"),
                message: error.to_string(),
            })
    }

    /// Loads, executes, and retains the stock FrameXML manifest.
    ///
    /// The supplied environment must already contain the selected character's
    /// synchronous world facts. This owner attaches the shared asset stack,
    /// profile CVars, AddOn catalog, and stock default binding image before Lua
    /// receives control.
    ///
    /// # Errors
    ///
    /// Returns [`GlueError`] at the first archive, XML, layout, object, font,
    /// texture, or Lua compatibility boundary.
    pub fn start_shared(
        assets: AssetStoreHandle,
        environment: UiScriptEnvironment,
        cvar_values: &[(String, String)],
        addon_catalog: &AddonCatalog,
    ) -> Result<Self, GlueError> {
        // 52A980 brackets the complete FrameXML load with 4CFB80/4CFB90.
        let _sound_admission = crate::script::UiSoundSuppression::new(environment.media_intent());
        let (catalog, bindings) = {
            let mut store = assets.borrow_mut();
            let catalog = UiBindingCatalog::load_builtin(&mut store)?;
            let bindings = UiBindingAssignments::load_defaults(&mut store, &catalog)?;
            (catalog, bindings)
        };
        let environment = environment
            .with_shared_asset_store(assets.clone())
            .with_cvar_values(cvar_values)
            .with_addon_load_state(crate::UiAddonLoadState::from_catalog(addon_catalog))
            .with_binding_assignments(bindings);
        let binding_assignments =
            environment
                .binding_assignments()
                .ok_or_else(|| crate::UiScriptError::Execution {
                    label: "FrameXML bindings".to_owned(),
                    message: "missing attached binding assignments".to_owned(),
                })?;
        let movement_input = environment.movement_input();
        let owner = GlueManager::start_shared_frame(assets, environment)?;
        let mut binding_functions = HashMap::new();
        for binding in catalog.bindings() {
            let function = owner
                .bundle()
                .lua()
                .load(format!(
                    "return function(keystate, pressure, angle, precision)\n{}\nend",
                    binding.body()
                ))
                .set_name(binding.name())
                .eval::<mlua::Function>()
                .map_err(|error| crate::UiScriptError::Execution {
                    label: binding.name().to_owned(),
                    message: error.to_string(),
                })?;
            binding_functions.insert(binding.name().to_owned(), function);
        }
        Ok(Self {
            owner,
            binding_catalog: catalog,
            binding_assignments,
            binding_functions,
            movement_input,
        })
    }

    /// Resolves physical input against the same assignment image used by Lua.
    /// The callback must finish before any resulting Lua command is executed.
    pub fn with_bindings<T>(
        &self,
        resolve: impl FnOnce(&UiBindingAssignments, &UiBindingCatalog) -> T,
    ) -> T {
        resolve(&self.binding_assignments.borrow(), &self.binding_catalog)
    }

    /// Publishes the source input clock before any focused handlers or bindings.
    pub fn set_input_event_time(&self, timestamp_ms: u32) {
        self.movement_input.set_event_time(timestamp_ms);
    }

    /// Takes the oldest native movement request, preserving its Lua-call time.
    pub fn take_movement_command(&self) -> Option<crate::UiMovementCommand> {
        self.movement_input.take()
    }

    /// Takes the oldest FrameXML sound operation for the process audio owner.
    pub fn take_media_action(&self) -> Option<crate::UiGlueMediaAction> {
        self.owner.take_media_action()
    }

    /// Refreshes physical output names after a device restart.
    pub fn set_sound_output_devices(&self, names: Vec<String>) {
        self.owner.set_sound_output_devices(names);
    }

    /// Publishes the device selected by the process sound owner.
    ///
    /// # Errors
    /// Returns an error if the registered output CVars cannot be updated.
    pub fn set_sound_output_selection(
        &self,
        index: usize,
        name: &str,
    ) -> Result<(), crate::UiScriptError> {
        self.owner.set_sound_output_selection(index, name)
    }

    /// Executes a stock command in the retained FrameXML Lua state and refreshes
    /// its presentation mutations. Returns whether presentation changed.
    ///
    /// # Errors
    /// Returns an error for an unavailable command, authored Lua failure, or
    /// invalid resulting presentation. Partial Lua mutations are retained.
    pub fn invoke_binding(&mut self, name: &str, pressed: bool) -> Result<bool, UiEventError> {
        let Some(definition) = self.binding_catalog.binding(name).filter(|definition| {
            !definition.is_debug() && definition.is_available_on(crate::UiBindingPlatform::Windows)
        }) else {
            return Err(crate::UiScriptError::Execution {
                label: name.to_owned(),
                message: "unavailable binding command".to_owned(),
            }
            .into());
        };
        if !pressed && !definition.runs_on_up() {
            return Ok(false);
        }
        let function = &self.binding_functions[name];
        self.owner.dispatch_binding(name, function, pressed)
    }

    /// Returns the upload-ready mesh rebuilt after each delivered event.
    #[must_use]
    pub const fn render_plan(&self) -> &UiRenderPlan {
        self.owner.render_plan()
    }

    /// Returns the current object name for a retained presentation index.
    #[must_use]
    pub fn object_name(&self, index: usize) -> Option<&str> {
        self.owner
            .objects()
            .get(index)
            .and_then(super::GlueObject::name)
    }

    /// Returns current resolved region geometry for the active interface.
    #[must_use]
    pub const fn geometry(&self) -> &crate::UiRegionGeometryPlan {
        self.owner.geometry()
    }

    /// Returns the active interface's source-preserving presentation packets.
    #[must_use]
    pub const fn presentation(&self) -> &crate::UiPresentationPlan {
        self.owner.presentation()
    }

    /// Returns one minimap's current widget settings and resolved viewport.
    #[must_use]
    pub fn minimap_presentation(
        &self,
        object_index: usize,
    ) -> Option<crate::UiMinimapPresentation> {
        self.owner.minimap_presentation(object_index)
    }

    /// Returns effective visibility of a named region, including its parents.
    /// Missing names or regions have no visibility result.
    #[must_use]
    pub fn region_is_shown(&self, name: &str) -> Option<bool> {
        let index = self
            .owner
            .objects()
            .iter()
            .position(|object| object.name() == Some(name))?;
        self.owner
            .geometry()
            .region(index)
            .map(crate::UiRegionGeometry::effectively_shown)
    }

    /// Returns the archive-font atlas retained across world frames.
    #[must_use]
    pub const fn glyphs(&self) -> &UiGlyphAtlasPlan {
        self.owner.glyphs()
    }

    /// Resolves every presentation-blocking BLP through the retained MPQ stack.
    ///
    /// # Errors
    ///
    /// Returns a render error when a required source cannot be read or parsed.
    pub fn load_blocking_render_textures(
        &self,
        cache: &mut BlpTextureCache,
    ) -> Result<UiTextureAssetBindings, crate::UiRenderError> {
        self.owner.load_blocking_render_textures(cache)
    }

    /// Delivers one canonical stock FrameXML event.
    ///
    /// # Errors
    ///
    /// Returns [`UiEventError`] when the event is unknown, authored Lua fails,
    /// or the resulting live presentation is invalid.
    pub fn dispatch_event(
        &mut self,
        name: &str,
        payload: &UiEventPayload,
    ) -> Result<UiEventDispatch, UiEventError> {
        // 528010 holds the same bracket on every world-entry notification,
        // including entry after a transfer when FrameXML is already resident.
        let _sound_admission =
            (name == "PLAYER_ENTERING_WORLD").then(|| self.owner.suppress_sound_entries());
        self.owner.dispatch_frame_event(name, payload)
    }

    /// Runs startup event publication under stock's non-positional sound gate.
    /// Nested transactions restore their enclosing gate even when Lua fails.
    pub fn with_suppressed_sound_entries<T>(&mut self, publish: impl FnOnce(&mut Self) -> T) -> T {
        let _sound_admission = self.owner.suppress_sound_entries();
        publish(self)
    }

    /// Retains a resolved gameplay combat event and delivers filtered and
    /// unfiltered notifications using the same positional argument image.
    ///
    /// # Errors
    /// Returns an event error when authored Lua or resulting presentation fails.
    pub fn append_combat_log(
        &mut self,
        entry: crate::UiCombatLogEntry,
    ) -> Result<(), UiEventError> {
        self.owner.append_combat_log(entry)
    }

    /// Advances visible FrameXML `OnUpdate` handlers.
    ///
    /// # Errors
    ///
    /// Returns [`UiEventError`] when the interval or resulting live state is
    /// invalid. Individual authored handler failures are isolated and
    /// available through [`Self::take_callback_failure`].
    pub fn update(&mut self, elapsed_seconds: f64) -> Result<bool, UiEventError> {
        self.owner.update(elapsed_seconds)
    }

    /// Takes one authored callback failure isolated from input and presentation.
    pub fn take_callback_failure(&mut self) -> Option<String> {
        self.owner.take_callback_failure()
    }

    /// Routes one logical UI pointer-button transition.
    ///
    /// # Errors
    ///
    /// Returns [`UiEventError`] when authored input Lua fails or leaves invalid
    /// presentation state.
    pub fn pointer_button(
        &mut self,
        position: (f64, f64),
        button: UiPointerButton,
        pressed: bool,
    ) -> Result<UiPointerDispatch, UiEventError> {
        self.owner.pointer_button(position, button, pressed)
    }

    /// Routes a pointer transition with the platform aggregate click count.
    pub fn pointer_button_with_click_count(
        &mut self,
        position: (f64, f64),
        button: UiPointerButton,
        pressed: bool,
        click_count: u8,
    ) -> Result<UiPointerDispatch, UiEventError> {
        self.owner
            .pointer_button_with_click_count(position, button, pressed, click_count)
    }

    /// Routes a pointer transition with its aggregate clicks and modifiers.
    ///
    /// # Errors
    ///
    /// Returns [`UiEventError`] when authored input Lua fails or leaves invalid
    /// presentation state.
    pub fn pointer_button_with_modifiers(
        &mut self,
        position: (f64, f64),
        button: UiPointerButton,
        pressed: bool,
        click_count: u8,
        modifiers: UiKeyboardModifiers,
    ) -> Result<UiPointerDispatch, UiEventError> {
        self.owner
            .pointer_button_with_modifiers(position, button, pressed, click_count, modifiers)
    }

    /// Updates pointer focus and any captured pointer drag.
    ///
    /// # Errors
    ///
    /// Returns [`UiEventError`] when authored slider Lua fails or leaves an
    /// invalid live presentation.
    pub fn pointer_motion(&mut self, position: (f64, f64)) -> Result<Option<usize>, UiEventError> {
        self.owner.pointer_motion(position)
    }

    /// Routes one wheel delta to the frontmost eligible FrameXML object.
    ///
    /// # Errors
    ///
    /// Returns [`UiEventError`] when authored wheel Lua fails or leaves an
    /// invalid live presentation.
    pub fn pointer_wheel(
        &mut self,
        position: (f64, f64),
        delta: f64,
    ) -> Result<Option<usize>, UiEventError> {
        self.owner.pointer_wheel(position, delta)
    }

    /// Delivers one normalized keyboard transition to the focused UI object.
    ///
    /// # Errors
    ///
    /// Returns [`UiEventError`] when authored keyboard Lua fails or leaves an
    /// invalid live presentation.
    pub fn keyboard_key(
        &mut self,
        key: &str,
        pressed: bool,
        modifiers: UiKeyboardModifiers,
    ) -> Result<Option<usize>, UiEventError> {
        self.owner.keyboard_key(key, pressed, modifiers)
    }

    /// Delivers committed platform text to the focused edit box.
    ///
    /// # Errors
    ///
    /// Returns [`UiEventError`] when authored edit-box Lua fails or leaves an
    /// invalid live presentation.
    pub fn text_input(&mut self, text: &str) -> Result<Option<usize>, UiEventError> {
        self.owner.text_input(text)
    }

    /// Delivers one uncommitted input-method composition.
    ///
    /// # Errors
    ///
    /// Returns [`UiEventError`] when authored composition Lua fails or leaves
    /// an invalid live presentation.
    pub fn text_composition(&mut self, text: &str) -> Result<Option<usize>, UiEventError> {
        self.owner.text_composition(text)
    }

    /// Returns the live object index of the focused visible edit box.
    #[must_use]
    pub fn focused_edit_box(&self) -> Option<usize> {
        self.owner.focused_edit_box()
    }

    /// Returns one registered FrameXML console variable's current text.
    #[must_use]
    pub fn cvar_value(&self, name: &str) -> Option<String> {
        self.owner.cvar_value(name)
    }

    /// Returns numeric subsystem policy without copying CVar text.
    #[must_use]
    pub fn cvar_number(&self, name: &str) -> Option<f32> {
        self.owner.cvar_number(name)
    }

    /// Returns the retained CVar generation for native policy caches.
    #[must_use]
    pub fn cvar_revision(&self) -> u64 {
        self.owner.cvar_revision()
    }

    /// Returns the shared minimap scene controls consumed by world rendering.
    #[must_use]
    pub fn minimap_state(&self) -> crate::UiMinimapState {
        self.owner.minimap_state()
    }

    /// Publishes an indoor/outdoor minimap transition before notifying FrameXML.
    ///
    /// # Errors
    /// Returns an event error when a `MINIMAP_UPDATE_ZOOM` handler fails.
    pub fn set_minimap_indoors(&mut self, indoors: bool) -> Result<(), UiEventError> {
        if self.minimap_state().set_indoors(indoors) {
            self.dispatch_event("MINIMAP_UPDATE_ZOOM", &UiEventPayload::default())?;
        }
        Ok(())
    }

    /// Takes profile-backed CVars changed by built-in FrameXML Lua.
    #[must_use]
    pub fn take_changed_cvars(&self) -> Vec<(String, String)> {
        self.owner.take_changed_cvars()
    }

    /// Takes the oldest process-lifetime action emitted by built-in FrameXML.
    #[must_use]
    pub fn take_process_action(&self) -> Option<UiProcessAction> {
        self.owner.take_process_action()
    }
}
