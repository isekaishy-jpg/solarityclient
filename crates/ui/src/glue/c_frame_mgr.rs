//! Persistent ownership of the built-in active-world interface.

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
}

impl FrameManager {
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
        let bindings = {
            let mut store = assets.borrow_mut();
            let catalog = UiBindingCatalog::load_builtin(&mut store)?;
            UiBindingAssignments::load_defaults(&mut store, &catalog)?
        };
        let environment = environment
            .with_shared_asset_store(assets.clone())
            .with_cvar_values(cvar_values)
            .with_addon_load_state(crate::UiAddonLoadState::from_catalog(addon_catalog))
            .with_binding_assignments(bindings);
        Ok(Self {
            owner: GlueManager::start_shared_frame(assets, environment)?,
        })
    }

    /// Returns the upload-ready mesh rebuilt after each delivered event.
    #[must_use]
    pub const fn render_plan(&self) -> &UiRenderPlan {
        self.owner.render_plan()
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
        self.owner.dispatch_frame_event(name, payload)
    }

    /// Advances visible FrameXML `OnUpdate` handlers.
    ///
    /// # Errors
    ///
    /// Returns [`UiEventError`] when the interval or resulting live state is
    /// invalid. Individual authored handler failures are isolated and
    /// available through [`Self::take_update_failure`].
    pub fn update(&mut self, elapsed_seconds: f64) -> Result<bool, UiEventError> {
        self.owner.update(elapsed_seconds)
    }

    /// Takes one authored `OnUpdate` failure isolated from frame presentation.
    pub fn take_update_failure(&mut self) -> Option<String> {
        self.owner.take_update_failure()
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
