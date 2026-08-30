//! Persistent ownership of the built-in login and character UI.

use std::cell::RefCell;
use std::rc::Rc;

use solarity_asset::AssetStore;

use crate::glue::{GlueError, GlueObject, GlueStartupReport};
use crate::script::UiRuntimeObjectPlan;
use crate::{
    FontCatalog, UiBundle, UiEventArgument, UiEventDispatch, UiEventError, UiEventPayload,
    UiFramePlan, UiFrameStatePlan, UiGlueMediaIntent, UiLayoutPlan, UiManifestKind,
    UiObjectCatalog, UiObjectTree, UiPresentationPlan, UiRegionGeometryPlan, UiRegionStatePlan,
    UiRuntimeTemplatePlan, UiScriptEnvironment, UiScriptPlan, UiScriptRuntime, UiScriptRuntimePlan,
    UiTexturePlan, UiTextureStatePlan,
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
    textures: UiTexturePlan,
    texture_states: UiTextureStatePlan,
    objects: Vec<GlueObject>,
    child_indices: Vec<usize>,
    report: GlueStartupReport,
    media_intent: Rc<RefCell<UiGlueMediaIntent>>,
    _assets: Rc<RefCell<AssetStore>>,
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
        mut assets: AssetStore,
        logical_extent: (u32, u32),
        streaming_trial: bool,
    ) -> Result<Self, GlueError> {
        let bundle = UiBundle::load(&mut assets, UiManifestKind::Glue)?;
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
        let assets = Rc::new(RefCell::new(assets));
        let environment =
            UiScriptEnvironment::new(logical_extent.0, logical_extent.1, streaming_trial)?
                .with_shared_asset_store(assets.clone());
        let media_intent = environment.media_intent();
        let ui_extent = environment.ui_extent();
        let runtime_plan = UiScriptRuntimePlan::new(
            &tree,
            &frames,
            &regions,
            &templates,
            &fonts,
            &texture_states,
        );
        let mut runtime = UiScriptRuntime::new(&bundle, &runtime_plan, environment)?;
        runtime.execute_all(&bundle, &tree, &scripts)?;
        runtime.dispatch_glue_event(&bundle, "FRAMES_LOADED", &UiEventPayload::empty())?;
        let login_payload = UiEventPayload::new([UiEventArgument::String("login".to_owned())])
            .map_err(|error| crate::UiScriptError::Plan {
                message: format!("could not construct stock login-screen event: {error}"),
            })?;
        runtime.dispatch_glue_event(&bundle, "SET_GLUE_SCREEN", &login_payload)?;

        let live = runtime.snapshot_objects(&bundle)?;
        let geometry = UiRegionGeometryPlan::resolve(&live, ui_extent)?;
        let presentation = UiPresentationPlan::resolve(&live, &geometry);
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
            textures,
            texture_states,
            objects,
            child_indices,
            report,
            media_intent,
            _assets: assets,
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

    /// Returns the stock music and ambience requests retained for `media`.
    #[must_use]
    pub fn media_intent(&self) -> UiGlueMediaIntent {
        self.media_intent.borrow().clone()
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

    fn refresh_live_state(&mut self) -> Result<(), UiEventError> {
        let live = self.runtime.snapshot_objects(&self.bundle)?;
        let geometry = UiRegionGeometryPlan::resolve(&live, self.geometry.ui_extent())?;
        let presentation = UiPresentationPlan::resolve(&live, &geometry);
        let (objects, child_indices) = build_live_hierarchy(&live)?;
        self.geometry = geometry;
        self.presentation = presentation;
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
