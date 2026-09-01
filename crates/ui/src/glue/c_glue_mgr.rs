//! Persistent ownership of the built-in login and character UI.

use std::cell::RefCell;
use std::rc::Rc;

use solarity_asset::{AssetStore, AssetStoreHandle, BlpTextureCache};

use crate::glue::{GlueError, GlueObject, GlueStartupReport};
use crate::script::UiGlueNetworkBridge;
use crate::script::UiRuntimeObjectPlan;
use crate::{
    FontCatalog, UiAnimationPlan, UiBundle, UiEventArgument, UiEventDispatch, UiEventError,
    UiEventPayload, UiFramePlan, UiFrameStatePlan, UiGlueMediaIntent, UiGlueNetworkAction,
    UiGlueNetworkStatus, UiLayoutPlan, UiManifestKind, UiObjectCatalog, UiObjectTree,
    UiPresentationPlan, UiRealmDirectory, UiRegionGeometryPlan, UiRegionStatePlan, UiRenderPlan,
    UiRuntimeTemplatePlan, UiScriptEnvironment, UiScriptPlan, UiScriptRuntime, UiScriptRuntimePlan,
    UiTextureAssetBindings, UiTexturePlan, UiTextureStatePlan,
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
    presentation: UiPresentationPlan,
    render_plan: UiRenderPlan,
    textures: UiTexturePlan,
    texture_states: UiTextureStatePlan,
    objects: Vec<GlueObject>,
    child_indices: Vec<usize>,
    report: GlueStartupReport,
    environment: UiScriptEnvironment,
    media_intent: Rc<RefCell<UiGlueMediaIntent>>,
    network: Rc<RefCell<UiGlueNetworkBridge>>,
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
        let bundle = UiBundle::load(&mut assets.borrow_mut(), UiManifestKind::Glue)?;
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
        let animations = UiAnimationPlan::from_tree(&tree)?;
        let environment =
            UiScriptEnvironment::new(logical_extent.0, logical_extent.1, streaming_trial)?
                .with_shared_asset_store(assets.clone())
                .with_cvar_values(cvar_values);
        let media_intent = environment.media_intent();
        let network = environment.network();
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
        runtime.dispatch_glue_event(&bundle, "FRAMES_LOADED", &UiEventPayload::empty())?;
        let initial_payload = UiEventPayload::new([UiEventArgument::String(
            initial_screen.script_name().to_owned(),
        )])
        .map_err(|error| crate::UiScriptError::Plan {
            message: format!("could not construct stock initial-screen event: {error}"),
        })?;
        runtime.dispatch_glue_event(&bundle, "SET_GLUE_SCREEN", &initial_payload)?;

        let live = runtime.snapshot_objects(&bundle)?;
        let geometry = UiRegionGeometryPlan::resolve(&live, ui_extent)?;
        let presentation = UiPresentationPlan::resolve(&live, &geometry);
        let render_plan = UiRenderPlan::prepare(&presentation, ui_extent)?;
        let (objects, child_indices) = build_live_hierarchy(&live)?;
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
            presentation,
            render_plan,
            textures,
            texture_states,
            objects,
            child_indices,
            report,
            environment,
            media_intent,
            network,
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

    /// Takes profile-backed CVars changed by built-in Glue Lua.
    pub fn take_changed_cvars(&self) -> Vec<(String, String)> {
        self.environment.take_changed_cvars()
    }

    /// Takes the oldest native network action emitted by built-in Glue Lua.
    #[must_use]
    pub fn take_network_action(&self) -> Option<UiGlueNetworkAction> {
        self.network.borrow_mut().take()
    }

    /// Publishes runtime-owned server facts for synchronous Glue queries.
    pub fn set_network_status(&self, status: UiGlueNetworkStatus) {
        self.network.borrow_mut().set_status(status);
    }

    /// Publishes the complete runtime-owned realm directory for Glue queries.
    pub fn set_realm_directory(&self, realms: UiRealmDirectory) {
        self.network.borrow_mut().set_realms(realms);
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

    /// Takes the native completion generated when authored Lua stops a movie.
    pub fn take_movie_stop_completion(&self) -> Option<usize> {
        self.media_intent.borrow_mut().take_movie_stop_completion()
    }

    fn refresh_live_state(&mut self) -> Result<(), UiEventError> {
        let live = self.runtime.snapshot_objects(&self.bundle)?;
        let geometry = UiRegionGeometryPlan::resolve(&live, self.geometry.ui_extent())?;
        let presentation = UiPresentationPlan::resolve(&live, &geometry);
        let render_plan = UiRenderPlan::prepare(&presentation, geometry.ui_extent())?;
        let (objects, child_indices) = build_live_hierarchy(&live)?;
        self.geometry = geometry;
        self.presentation = presentation;
        self.render_plan = render_plan;
        self.objects = objects;
        self.child_indices = child_indices;
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
