//! Persistent ownership of the built-in login and character UI.

use std::cell::RefCell;
use std::rc::Rc;

use solarity_asset::{AssetStore, AssetStoreHandle, BlpTextureCache};
use solarity_cpu::BlizzardRand;

use crate::glue::pointer::UiPointerPlan;
use crate::glue::{GlueError, GlueObject, GlueStartupReport};
use crate::script::UiRuntimeObjectPlan;
use crate::script::{UiGlueNetworkBridge, UiProcessBridge};
use crate::{
    FontCatalog, UiAnimationPlan, UiBackdropPlan, UiBackdropStatePlan, UiBundle, UiEventArgument,
    UiEventDispatch, UiEventError, UiEventPayload, UiFramePlan, UiFrameStatePlan,
    UiGlueMediaIntent, UiGlueNetworkAction, UiGlueNetworkStatus, UiGlyphAtlasPlan,
    UiKeyboardModifiers, UiLayoutPlan, UiManifestKind, UiObjectCatalog, UiObjectKind, UiObjectTree,
    UiPointerButton, UiPointerDispatch, UiPresentationPlan, UiRealmDirectory, UiRegionGeometryPlan,
    UiRegionStatePlan, UiRenderPlan, UiRuntimeTemplatePlan, UiScriptEnvironment, UiScriptPlan,
    UiScriptRuntime, UiScriptRuntimePlan, UiScrollFramePlan, UiTextureAssetBindings, UiTexturePlan,
    UiTextureStatePlan,
};

/// Complete built-in GlueXML state retained across the pre-world lifetime.
///
/// Runtime and registry-key fields precede `bundle` deliberately: Rust drops
/// fields in declaration order, so every Lua registry key is released before
/// the bundle-owned Lua 5.1 state.
pub struct GlueManager {
    runtime: UiScriptRuntime,
    scripts: UiScriptPlan,
    templates: UiRuntimeTemplatePlan,
    fonts: FontCatalog,
    frames: UiFrameStatePlan,
    regions: UiRegionStatePlan,
    geometry: UiRegionGeometryPlan,
    scroll_frames: UiScrollFramePlan,
    glyphs: UiGlyphAtlasPlan,
    presentation: UiPresentationPlan,
    render_plan: UiRenderPlan,
    textures: UiTexturePlan,
    texture_states: UiTextureStatePlan,
    backdrops: UiBackdropStatePlan,
    objects: Vec<GlueObject>,
    child_indices: Vec<usize>,
    pointer: UiPointerPlan,
    pointer_capture: Option<(usize, UiPointerButton)>,
    glyph_logical_height: u32,
    report: GlueStartupReport,
    environment: UiScriptEnvironment,
    media_intent: Rc<RefCell<UiGlueMediaIntent>>,
    network: Rc<RefCell<UiGlueNetworkBridge>>,
    process: Rc<RefCell<UiProcessBridge>>,
    assets: AssetStoreHandle,
    bundle: UiBundle,
}

impl GlueManager {
    /// Loads, plans, and executes the stock GlueXML manifest in source order.
    ///
    /// The supplied archive stack becomes the single persistent asset source
    /// used by model and font Lua methods; startup does not remount the MPQs.
    ///
    /// # Errors
    ///
    /// Returns [`GlueError`] at the first archive, XML, layout, object, font,
    /// texture, or Lua compatibility boundary.
    pub fn start(
        assets: AssetStore,
        logical_extent: (u32, u32),
        streaming_trial: bool,
    ) -> Result<Self, GlueError> {
        Self::start_shared(
            AssetStoreHandle::new(assets),
            logical_extent,
            streaming_trial,
        )
    }

    /// Starts Glue against the process-wide main-thread asset stack.
    ///
    /// # Errors
    ///
    /// Returns [`GlueError`] under the same strict stock-loading rules as
    /// [`Self::start`].
    pub fn start_shared(
        assets: AssetStoreHandle,
        logical_extent: (u32, u32),
        streaming_trial: bool,
    ) -> Result<Self, GlueError> {
        Self::start_shared_with_initial_screen(
            assets,
            logical_extent,
            streaming_trial,
            super::GlueInitialScreen::Login,
        )
    }

    /// Starts Glue on the native screen selected from persistent startup state.
    ///
    /// # Errors
    ///
    /// Returns [`GlueError`] under the same strict stock-loading rules as
    /// [`Self::start_shared`].
    pub fn start_shared_with_initial_screen(
        assets: AssetStoreHandle,
        logical_extent: (u32, u32),
        streaming_trial: bool,
        initial_screen: super::GlueInitialScreen,
    ) -> Result<Self, GlueError> {
        Self::start_shared_with_initial_screen_and_cvars(
            assets,
            logical_extent,
            streaming_trial,
            initial_screen,
            &[],
        )
    }

    /// Starts Glue with profile CVar values loaded before authored Lua runs.
    ///
    /// # Errors
    ///
    /// Returns [`GlueError`] under the same strict stock-loading rules as
    /// [`Self::start_shared_with_initial_screen`].
    pub fn start_shared_with_initial_screen_and_cvars(
        assets: AssetStoreHandle,
        logical_extent: (u32, u32),
        streaming_trial: bool,
        initial_screen: super::GlueInitialScreen,
        cvar_values: &[(String, String)],
    ) -> Result<Self, GlueError> {
        Self::start_shared_with_profile(
            assets,
            logical_extent,
            streaming_trial,
            initial_screen,
            cvar_values,
            &crate::AddonCatalog::default(),
        )
    }

    /// Starts Glue with persistent profile values and the discovered AddOn catalog.
    ///
    /// # Errors
    ///
    /// Returns [`GlueError`] under the same strict stock-loading rules as
    /// [`Self::start_shared_with_initial_screen_and_cvars`].
    pub fn start_shared_with_profile(
        assets: AssetStoreHandle,
        logical_extent: (u32, u32),
        streaming_trial: bool,
        initial_screen: super::GlueInitialScreen,
        cvar_values: &[(String, String)],
        addon_catalog: &crate::AddonCatalog,
    ) -> Result<Self, GlueError> {
        Self::start_shared_with_profile_and_character_creation(
            assets,
            logical_extent,
            streaming_trial,
            initial_screen,
            cvar_values,
            addon_catalog,
            None,
        )
    }

    /// Starts production Glue with DBC-backed character creation and the
    /// process-wide Blizzard random stream.
    ///
    /// The simpler constructors intentionally omit this state so focused Glue
    /// fixture tests do not have to fabricate unrelated client databases.
    ///
    /// # Errors
    ///
    /// Returns [`GlueError`] under the same strict stock-loading rules as
    /// [`Self::start_shared_with_profile`], including malformed character DBCs.
    pub fn start_shared_with_profile_and_random(
        assets: AssetStoreHandle,
        logical_extent: (u32, u32),
        streaming_trial: bool,
        initial_screen: super::GlueInitialScreen,
        cvar_values: &[(String, String)],
        addon_catalog: &crate::AddonCatalog,
        random: Rc<RefCell<BlizzardRand>>,
    ) -> Result<Self, GlueError> {
        let character_creation = crate::UiCharacterCreationState::load(
            &mut assets.borrow_mut(),
            streaming_trial,
            random,
        )?;
        Self::start_shared_with_profile_and_character_creation(
            assets,
            logical_extent,
            streaming_trial,
            initial_screen,
            cvar_values,
            addon_catalog,
            Some(character_creation),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn start_shared_with_profile_and_character_creation(
        assets: AssetStoreHandle,
        logical_extent: (u32, u32),
        streaming_trial: bool,
        initial_screen: super::GlueInitialScreen,
        cvar_values: &[(String, String)],
        addon_catalog: &crate::AddonCatalog,
        character_creation: Option<crate::UiCharacterCreationState>,
    ) -> Result<Self, GlueError> {
        let mut environment =
            UiScriptEnvironment::new(logical_extent.0, logical_extent.1, streaming_trial)?
                .with_shared_asset_store(assets.clone())
                .with_cvar_values(cvar_values)
                .with_addon_load_state(crate::UiAddonLoadState::from_catalog(addon_catalog));
        if let Some(character_creation) = character_creation {
            environment = environment.with_character_creation_state(character_creation);
        }
        Self::start_shared_owner(
            assets,
            environment,
            UiManifestKind::Glue,
            Some(initial_screen),
        )
    }

    /// Constructs one independently owned active-world FrameXML runtime.
    pub(super) fn start_shared_frame(
        assets: AssetStoreHandle,
        environment: UiScriptEnvironment,
    ) -> Result<Self, GlueError> {
        Self::start_shared_owner(assets, environment, UiManifestKind::Frame, None)
    }

    /// Builds common retained script, object, layout, and rendering state.
    fn start_shared_owner(
        assets: AssetStoreHandle,
        environment: UiScriptEnvironment,
        manifest_kind: UiManifestKind,
        initial_screen: Option<super::GlueInitialScreen>,
    ) -> Result<Self, GlueError> {
        let logical_extent = environment.logical_extent();
        let bundle = UiBundle::load(&mut assets.borrow_mut(), manifest_kind)?;
        let fonts = FontCatalog::from_bundle(&bundle)?;
        let catalog = UiObjectCatalog::from_bundle(&bundle, &fonts)?;
        let tree = UiObjectTree::from_catalog(&catalog, &fonts)?;
        let frame_plan = UiFramePlan::from_tree(&tree)?;
        let frames = frame_plan.resolve(&tree)?;
        let layout = UiLayoutPlan::from_tree(&tree)?;
        let regions = UiRegionStatePlan::resolve(&tree, &layout)?;
        let scripts = UiScriptPlan::from_tree(&tree, bundle.lua())?;
        let templates = UiRuntimeTemplatePlan::from_catalog(&catalog, &fonts, bundle.lua())?;
        let textures = UiTexturePlan::from_tree(&tree)?;
        let texture_states = UiTextureStatePlan::resolve(&tree, &textures)?;
        let backdrop_plan = UiBackdropPlan::from_tree(&tree)?;
        let backdrops = UiBackdropStatePlan::resolve(&tree, &backdrop_plan)?;
        let animations = UiAnimationPlan::from_tree(&tree)?;
        let media_intent = environment.media_intent();
        let network = environment.network();
        let process = environment.process();
        let ui_extent = environment.ui_extent();
        let runtime_plan = UiScriptRuntimePlan::new(
            &tree,
            &animations,
            &frames,
            &regions,
            &templates,
            &fonts,
            &texture_states,
        );
        let mut runtime = UiScriptRuntime::new(&bundle, &runtime_plan, environment.clone())?;
        runtime.execute_all(&bundle, &tree, &scripts)?;
        if let Some(initial_screen) = initial_screen {
            runtime.dispatch_glue_event(&bundle, "FRAMES_LOADED", &UiEventPayload::empty())?;
            let initial_payload = UiEventPayload::new([UiEventArgument::String(
                initial_screen.script_name().to_owned(),
            )])
            .map_err(|error| crate::UiScriptError::Plan {
                message: format!("could not construct stock initial-screen event: {error}"),
            })?;
            runtime.dispatch_glue_event(&bundle, "SET_GLUE_SCREEN", &initial_payload)?;
        }

        let live = runtime.snapshot_objects(&bundle)?;
        let geometry = UiRegionGeometryPlan::resolve(&live, ui_extent)?;
        runtime.publish_resolved_geometry(&bundle, &geometry)?;
        let scroll_frames = UiScrollFramePlan::from_live(&live);
        let glyphs = UiGlyphAtlasPlan::from_live_ui(
            runtime.simple_html(),
            &live,
            &geometry,
            &fonts,
            &mut assets.borrow_mut(),
            logical_extent.1,
        )?;
        let presentation = UiPresentationPlan::resolve(&live, &geometry, &backdrops);
        let render_plan = UiRenderPlan::prepare_with_glyphs(
            &presentation,
            &glyphs,
            &geometry,
            &scroll_frames,
            ui_extent,
        )?;
        let (objects, child_indices) = build_live_hierarchy(&live)?;
        let pointer = UiPointerPlan::from_live(&live);
        let report = GlueStartupReport::new(
            bundle.resources().len(),
            bundle.actions().len(),
            objects.len(),
            objects
                .iter()
                .filter(|object| object.name().is_some())
                .count(),
            frames.state_count(),
            geometry.region_count(),
            textures.layer_count(),
            runtime.executed_chunk_count(),
            runtime.executed_load_handler_count(),
        );
        Ok(Self {
            runtime,
            scripts,
            templates,
            fonts,
            frames,
            regions,
            geometry,
            scroll_frames,
            glyphs,
            presentation,
            render_plan,
            textures,
            texture_states,
            backdrops,
            objects,
            child_indices,
            pointer,
            pointer_capture: None,
            glyph_logical_height: logical_extent.1,
            report,
            environment,
            media_intent,
            network,
            process,
            assets,
            bundle,
        })
    }

    /// Returns immutable startup evidence for diagnostics and loading screens.
    #[must_use]
    pub const fn report(&self) -> GlueStartupReport {
        self.report
    }

    /// Returns the loaded stock manifest and its Lua state.
    #[must_use]
    pub const fn bundle(&self) -> &UiBundle {
        &self.bundle
    }

    /// Returns persistent global font definitions.
    #[must_use]
    pub const fn fonts(&self) -> &FontCatalog {
        &self.fonts
    }

    /// Returns compact live-object identity and ownership state.
    #[must_use]
    pub fn objects(&self) -> &[GlueObject] {
        &self.objects
    }

    /// Returns direct children for one object in stock construction order.
    #[must_use]
    pub fn children(&self, object_index: usize) -> Option<&[usize]> {
        let object = self.objects.get(object_index)?;
        Some(&self.child_indices[object.first_child()..object.first_child() + object.child_count()])
    }

    /// Returns resolved frame ordering and interaction properties.
    #[must_use]
    pub const fn frames(&self) -> &UiFrameStatePlan {
        &self.frames
    }

    /// Returns resolved dimensions, visibility, alpha, scale, and anchors.
    #[must_use]
    pub const fn regions(&self) -> &UiRegionStatePlan {
        &self.regions
    }

    /// Returns live rectangles resolved after startup Lua has run.
    #[must_use]
    pub const fn geometry(&self) -> &UiRegionGeometryPlan {
        &self.geometry
    }

    /// Returns live ScrollFrame offsets and ranges parallel to Glue objects.
    #[must_use]
    pub const fn scroll_frames(&self) -> &UiScrollFramePlan {
        &self.scroll_frames
    }

    /// Returns the immutable archive-font atlas retained across Glue screens.
    #[must_use]
    pub const fn glyphs(&self) -> &UiGlyphAtlasPlan {
        &self.glyphs
    }

    /// Returns post-Lua texture packets in deterministic stock draw order.
    #[must_use]
    pub const fn presentation(&self) -> &UiPresentationPlan {
        &self.presentation
    }

    /// Returns the upload-ready mesh rebuilt after each delivered Glue event.
    #[must_use]
    pub const fn render_plan(&self) -> &UiRenderPlan {
        &self.render_plan
    }

    /// Returns the locale-loaded legal and help document layout.
    #[must_use]
    pub const fn simple_html(&self) -> &crate::UiSimpleHtmlPlan {
        self.runtime.simple_html()
    }

    /// Resolves every presentation-blocking BLP through the retained MPQ stack.
    ///
    /// # Errors
    ///
    /// Returns [`crate::UiRenderError::Asset`] when a required source cannot be
    /// read or parsed. Non-blocking sources remain pending for the streamer.
    pub fn load_blocking_render_textures(
        &self,
        cache: &mut BlpTextureCache,
    ) -> Result<UiTextureAssetBindings, crate::UiRenderError> {
        self.render_plan
            .texture_assets()
            .load_blocking(&mut self.assets.borrow_mut(), cache)
    }

    /// Returns the stock music and ambience requests retained for `media`.
    #[must_use]
    pub fn media_intent(&self) -> UiGlueMediaIntent {
        self.media_intent.borrow().clone()
    }

    /// Returns the native Glue screen name mirrored by `SetCurrentScreen`.
    #[must_use]
    pub fn current_screen(&self) -> String {
        self.environment.current_screen().borrow().clone()
    }

    /// Publishes the authenticated account entitlement used by character creation.
    pub fn set_character_creation_expansion(&self, expansion: crate::UiCharacterExpansion) {
        if let Some(state) = self.environment.character_creation_state() {
            state.set_expansion(expansion);
        }
    }

    /// Returns the current DBC-backed character preview when production Glue
    /// attached character-creation state.
    #[must_use]
    pub fn character_creation_preview(&self) -> Option<crate::UiCharacterCreationPreview> {
        self.environment
            .character_creation_state()
            .map(|state| state.preview())
    }

    /// Returns the currently selected enumeration row as renderer inputs.
    #[must_use]
    pub fn character_selection_preview(&self) -> Option<crate::UiCharacterSelectionPreview> {
        self.environment
            .network()
            .borrow()
            .character_selection_preview()
    }

    /// Returns the cursor-visibility request authored by the current screen.
    #[must_use]
    pub fn cursor_visible(&self) -> bool {
        self.environment.cursor_visible().get()
    }

    /// Returns one live script-visible CVar for native subsystem policy.
    #[must_use]
    pub fn cvar_value(&self, name: &str) -> Option<String> {
        self.environment.cvar_value(name)
    }

    /// Applies stock's numeric truth test to one live console variable.
    #[must_use]
    pub fn cvar_boolean(&self, name: &str) -> bool {
        self.environment.cvar_boolean(name)
    }

    /// Resolves one localization token from the loaded Glue Lua globals.
    ///
    /// # Errors
    ///
    /// Returns [`crate::UiScriptError`] when Lua cannot read the global table.
    pub fn localized_text(&self, token: &str) -> Result<String, crate::UiScriptError> {
        self.bundle
            .lua()
            .globals()
            .raw_get::<Option<String>>(token)
            .map(|value| {
                value
                    .filter(|text| !text.is_empty())
                    .unwrap_or_else(|| token.to_owned())
            })
            .map_err(|error| crate::UiScriptError::Execution {
                label: format!("localization token {token}"),
                message: error.to_string(),
            })
    }

    /// Takes profile-backed CVars changed by built-in Glue Lua.
    pub fn take_changed_cvars(&self) -> Vec<(String, String)> {
        self.environment.take_changed_cvars()
    }

    /// Takes the oldest native network action emitted by built-in Glue Lua.
    #[must_use]
    pub fn take_network_action(&self) -> Option<UiGlueNetworkAction> {
        self.network.borrow_mut().take()
    }

    /// Takes the oldest process-lifetime action emitted by built-in UI Lua.
    #[must_use]
    pub fn take_process_action(&self) -> Option<crate::UiProcessAction> {
        self.process.borrow_mut().take()
    }

    /// Publishes runtime-owned server facts for synchronous Glue queries.
    pub fn set_network_status(&self, status: UiGlueNetworkStatus) {
        self.network.borrow_mut().set_status(status);
    }

    /// Publishes the complete runtime-owned realm directory for Glue queries.
    pub fn set_realm_directory(&self, realms: UiRealmDirectory) {
        self.network.borrow_mut().set_realms(realms);
    }

    /// Applies one stock realm-list sort generation before Glue redraws it.
    pub fn sort_realm_directory(&self, sort: crate::UiRealmSort) {
        self.network.borrow_mut().realms_mut().sort(sort);
    }

    /// Publishes the complete runtime-owned character directory for Glue queries.
    pub fn set_character_directory(&self, characters: crate::UiCharacterDirectory) {
        self.network.borrow_mut().set_characters(characters);
    }

    /// Returns the archive-backed texture declaration plan.
    #[must_use]
    pub const fn textures(&self) -> &UiTexturePlan {
        &self.textures
    }

    /// Returns declaration-resolved startup state for static texture objects.
    #[must_use]
    pub const fn texture_states(&self) -> &UiTextureStatePlan {
        &self.texture_states
    }

    /// Returns declaration-resolved native frame backdrops.
    #[must_use]
    pub const fn backdrops(&self) -> &UiBackdropStatePlan {
        &self.backdrops
    }

    /// Returns compiled event and handler functions retained in Lua.
    #[must_use]
    pub const fn scripts(&self) -> &UiScriptPlan {
        &self.scripts
    }

    /// Returns runtime templates retained for dynamic `CreateFrame` calls.
    #[must_use]
    pub const fn templates(&self) -> &UiRuntimeTemplatePlan {
        &self.templates
    }

    /// Delivers a stock Glue event to registered frames in creation order.
    ///
    /// # Errors
    ///
    /// Returns [`UiEventError::Unknown`] for an event outside build 12340's
    /// Glue registry or [`UiEventError::Script`] when a handler fails.
    pub fn dispatch_event(
        &mut self,
        name: &str,
        payload: &UiEventPayload,
    ) -> Result<UiEventDispatch, UiEventError> {
        let event =
            crate::event::canonical_glue_event(name).ok_or_else(|| UiEventError::Unknown {
                name: name.to_owned(),
            })?;
        let subscriber_count = self
            .runtime
            .dispatch_glue_event(&self.bundle, event, payload)?;
        self.refresh_live_state()?;
        Ok(UiEventDispatch::new(subscriber_count))
    }

    /// Delivers a stock FrameXML event to registered frames in creation order.
    pub(super) fn dispatch_frame_event(
        &mut self,
        name: &str,
        payload: &UiEventPayload,
    ) -> Result<UiEventDispatch, UiEventError> {
        let event =
            crate::event::canonical_frame_event(name).ok_or_else(|| UiEventError::Unknown {
                name: name.to_owned(),
            })?;
        let subscriber_count = self
            .runtime
            .dispatch_glue_event(&self.bundle, event, payload)?;
        self.refresh_live_state()?;
        Ok(UiEventDispatch::new(subscriber_count))
    }

    /// Advances visible Glue `OnUpdate` handlers by one rendered-frame interval.
    ///
    /// Live geometry and renderer packets rebuild only when a handler mutates
    /// presentation state, so idle Glue frames do not recreate the UI mesh.
    ///
    /// # Errors
    ///
    /// Returns [`UiEventError`] when the interval is invalid, a visible update
    /// handler fails, or its mutations cannot form valid presentation state.
    pub fn update(&mut self, elapsed_seconds: f64) -> Result<bool, UiEventError> {
        let (_handler_count, changed) = self
            .runtime
            .dispatch_updates(&self.bundle, elapsed_seconds)?;
        if changed {
            self.refresh_live_state()?;
        }
        Ok(changed)
    }

    /// Delivers native decode completion to the owning stock `MovieFrame`.
    ///
    /// # Errors
    ///
    /// Returns [`UiEventError`] when the object is unavailable, its authored
    /// `OnMovieFinished` handler fails, or the resulting live layout is invalid.
    pub fn movie_finished(&mut self, object_index: usize) -> Result<(), UiEventError> {
        {
            let mut media = self.media_intent.borrow_mut();
            media.retire_movie(object_index);
        }
        self.runtime
            .dispatch_movie_finished(&self.bundle, object_index)?;
        self.refresh_live_state()
    }

    /// Delivers a stock-normalized released key to the active MovieFrame.
    ///
    /// # Errors
    ///
    /// Returns [`UiEventError`] when the object is unavailable, its authored
    /// `OnKeyUp` handler fails, or the resulting live layout is invalid.
    pub fn movie_key_up(&mut self, object_index: usize, key: &str) -> Result<(), UiEventError> {
        self.runtime
            .dispatch_movie_key_up(&self.bundle, object_index, key)?;
        self.refresh_live_state()
    }

    /// Routes one bottom-left-origin logical UI pointer transition.
    ///
    /// Press selects the frontmost enabled mouse frame by stratum, level, and
    /// construction order. Release remains captured by that button, while its
    /// click callback activates only when the pointer is still over the same
    /// target and that exact transition was registered.
    ///
    /// # Errors
    ///
    /// Returns [`UiEventError`] when authored pointer or click Lua fails, or
    /// when the resulting live geometry/presentation state is invalid.
    pub fn pointer_button(
        &mut self,
        position: (f64, f64),
        button: UiPointerButton,
        pressed: bool,
    ) -> Result<UiPointerDispatch, UiEventError> {
        let hit = self.pointer.hit_test(&self.geometry, position);
        self.environment.mouse_focus().set(hit);
        let object_index = if pressed {
            hit
        } else {
            self.pointer_capture
                .take()
                .filter(|(_, captured_button)| *captured_button == button)
                .map(|(index, _)| index)
        };
        let Some(object_index) = object_index else {
            return Ok(UiPointerDispatch::new(None, false));
        };
        if pressed {
            self.pointer_capture = Some((object_index, button));
        }
        let kind = self
            .pointer
            .kind(object_index)
            .ok_or_else(|| crate::UiScriptError::Plan {
                message: format!("pointer target {object_index} lost its live frame state"),
            })?;
        let click_activated = match kind {
            UiObjectKind::Button | UiObjectKind::CheckButton => {
                let activate = (!pressed && hit == Some(object_index) || pressed)
                    && self.pointer.activates(object_index, button, pressed);
                self.runtime.dispatch_button_pointer(
                    &self.bundle,
                    object_index,
                    button.script_name(),
                    pressed,
                    activate,
                )?;
                activate
            }
            UiObjectKind::EditBox => {
                if pressed {
                    self.runtime.focus_edit_box(&self.bundle, object_index)?;
                }
                self.runtime.dispatch_frame_pointer(
                    &self.bundle,
                    object_index,
                    button.script_name(),
                    pressed,
                )?;
                false
            }
            UiObjectKind::Slider => {
                if button == UiPointerButton::Left
                    && let Some(value) =
                        self.pointer
                            .slider_value_at(&self.geometry, object_index, position)
                {
                    self.runtime
                        .dispatch_slider_value(&self.bundle, object_index, value)?;
                }
                self.runtime.dispatch_frame_pointer(
                    &self.bundle,
                    object_index,
                    button.script_name(),
                    pressed,
                )?;
                false
            }
            _ => false,
        };
        self.refresh_live_state()?;
        Ok(UiPointerDispatch::new(Some(object_index), click_activated))
    }

    /// Updates a captured Slider from one bottom-left-origin pointer position.
    ///
    /// # Errors
    ///
    /// Returns [`UiEventError`] when the slider's `OnValueChanged` handler
    /// fails or the resulting live presentation cannot be resolved.
    pub fn pointer_motion(&mut self, position: (f64, f64)) -> Result<Option<usize>, UiEventError> {
        self.environment
            .mouse_focus()
            .set(self.pointer.hit_test(&self.geometry, position));
        let Some((object_index, UiPointerButton::Left)) = self.pointer_capture else {
            return Ok(None);
        };
        if self.pointer.kind(object_index) != Some(UiObjectKind::Slider) {
            return Ok(None);
        }
        let Some(value) = self
            .pointer
            .slider_value_at(&self.geometry, object_index, position)
        else {
            return Ok(None);
        };
        self.runtime
            .dispatch_slider_value(&self.bundle, object_index, value)?;
        self.refresh_live_state()?;
        Ok(Some(object_index))
    }

    /// Returns the live object index of the focused visible EditBox.
    #[must_use]
    pub fn focused_edit_box(&self) -> Option<usize> {
        self.pointer.focused_edit_box(&self.geometry)
    }

    /// Delivers committed platform text to the focused stock EditBox.
    ///
    /// # Errors
    ///
    /// Returns [`UiEventError`] when authored `OnTextChanged` or `OnChar` Lua
    /// fails, or when the resulting live UI state cannot be resolved.
    pub fn text_input(&mut self, text: &str) -> Result<Option<usize>, UiEventError> {
        let Some(object_index) = self.focused_edit_box() else {
            return Ok(None);
        };
        self.runtime
            .dispatch_edit_text(&self.bundle, object_index, text)?;
        self.refresh_live_state()?;
        Ok(Some(object_index))
    }

    /// Delivers one uncommitted input-method composition to the focused EditBox.
    ///
    /// # Errors
    ///
    /// Returns [`UiEventError`] when authored `OnCharComposition` Lua fails.
    pub fn text_composition(&mut self, text: &str) -> Result<Option<usize>, UiEventError> {
        let Some(object_index) = self.focused_edit_box() else {
            return Ok(None);
        };
        self.runtime
            .dispatch_edit_composition(&self.bundle, object_index, text)?;
        self.refresh_live_state()?;
        Ok(Some(object_index))
    }

    /// Delivers one stock key transition to the focused EditBox or top frame.
    ///
    /// # Errors
    ///
    /// Returns [`UiEventError`] when an authored keyboard or EditBox callback
    /// fails, or when the resulting live UI state cannot be resolved.
    pub fn keyboard_key(
        &mut self,
        key: &str,
        pressed: bool,
        modifiers: UiKeyboardModifiers,
    ) -> Result<Option<usize>, UiEventError> {
        let target = self
            .focused_edit_box()
            .or_else(|| self.pointer.keyboard_target(&self.geometry));
        let Some(object_index) = target else {
            return Ok(None);
        };
        self.runtime
            .dispatch_keyboard_key(&self.bundle, object_index, key, pressed, modifiers)?;
        self.refresh_live_state()?;
        Ok(Some(object_index))
    }

    /// Routes one normalized wheel delta to the frontmost ScrollFrame.
    ///
    /// # Errors
    ///
    /// Returns [`UiEventError`] when authored `OnMouseWheel`, slider, or
    /// `OnVerticalScroll` Lua fails, or leaves invalid live presentation state.
    pub fn pointer_wheel(
        &mut self,
        position: (f64, f64),
        delta: f64,
    ) -> Result<Option<usize>, UiEventError> {
        let Some(object_index) = self.pointer.wheel_hit_test(&self.geometry, position) else {
            return Ok(None);
        };
        self.runtime
            .dispatch_mouse_wheel(&self.bundle, object_index, delta)?;
        self.refresh_live_state()?;
        Ok(Some(object_index))
    }

    /// Takes the native completion generated when authored Lua stops a movie.
    pub fn take_movie_stop_completion(&self) -> Option<usize> {
        self.media_intent.borrow_mut().take_movie_stop_completion()
    }

    fn refresh_live_state(&mut self) -> Result<(), UiEventError> {
        let live = self.runtime.snapshot_objects(&self.bundle)?;
        let geometry = UiRegionGeometryPlan::resolve(&live, self.geometry.ui_extent())?;
        self.runtime
            .publish_resolved_geometry(&self.bundle, &geometry)?;
        let scroll_frames = UiScrollFramePlan::from_live(&live);
        if self
            .glyphs
            .supports_live_text(&live, self.glyph_logical_height)
        {
            self.glyphs
                .refresh_live_text(&live, &geometry, self.glyph_logical_height)?;
        } else {
            self.glyphs = UiGlyphAtlasPlan::from_live_ui(
                self.runtime.simple_html(),
                &live,
                &geometry,
                &self.fonts,
                &mut self.assets.borrow_mut(),
                self.glyph_logical_height,
            )?;
        }
        let presentation = UiPresentationPlan::resolve(&live, &geometry, &self.backdrops);
        let render_plan = UiRenderPlan::prepare_with_glyphs(
            &presentation,
            &self.glyphs,
            &geometry,
            &scroll_frames,
            geometry.ui_extent(),
        )?;
        let (objects, child_indices) = build_live_hierarchy(&live)?;
        let pointer = UiPointerPlan::from_live(&live);
        self.geometry = geometry;
        self.scroll_frames = scroll_frames;
        self.presentation = presentation;
        self.render_plan = render_plan;
        self.objects = objects;
        self.child_indices = child_indices;
        self.pointer = pointer;
        Ok(())
    }
}

fn build_live_hierarchy(
    live: &UiRuntimeObjectPlan,
) -> Result<(Vec<GlueObject>, Vec<usize>), crate::UiScriptError> {
    let mut children = vec![Vec::new(); live.objects().len()];
    for (index, object) in live.objects().iter().enumerate() {
        if let Some(parent) = object.parent {
            let Some(owner) = children.get_mut(parent) else {
                return Err(crate::UiScriptError::Plan {
                    message: format!("live UI object {index} has unavailable parent {parent}"),
                });
            };
            owner.push(index);
        }
    }
    let child_count = children.iter().map(Vec::len).sum();
    let mut child_indices = Vec::with_capacity(child_count);
    let objects = live
        .objects()
        .iter()
        .zip(children)
        .map(|(object, children)| {
            let first_child = child_indices.len();
            let child_count = children.len();
            child_indices.extend(children);
            GlueObject::from_runtime(object, first_child, child_count)
        })
        .collect();
    Ok((objects, child_indices))
}
