//! Persistent ownership of the built-in login and character UI.

use std::cell::RefCell;
use std::rc::Rc;

use solarity_asset::AssetStore;

use crate::glue::{GlueError, GlueObject, GlueStartupReport};
use crate::{
    FontCatalog, UiBundle, UiFramePlan, UiFrameStatePlan, UiLayoutPlan, UiManifestKind,
    UiObjectCatalog, UiObjectTree, UiRegionStatePlan, UiRuntimeTemplatePlan, UiScriptEnvironment,
    UiScriptPlan, UiScriptRuntime, UiScriptRuntimePlan, UiTexturePlan,
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
    textures: UiTexturePlan,
    objects: Vec<GlueObject>,
    child_indices: Vec<usize>,
    report: GlueStartupReport,
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
        let assets = Rc::new(RefCell::new(assets));
        let environment =
            UiScriptEnvironment::new(logical_extent.0, logical_extent.1, streaming_trial)?
                .with_shared_asset_store(assets.clone());
        let runtime_plan =
            UiScriptRuntimePlan::new(&tree, &frames, &regions, &templates, &fonts, &textures);
        let mut runtime = UiScriptRuntime::new(&bundle, &runtime_plan, environment)?;
        runtime.execute_all(&bundle, &tree, &scripts)?;

        let child_count = tree.nodes().iter().map(|node| node.children().len()).sum();
        let mut child_indices = Vec::with_capacity(child_count);
        let objects = tree
            .nodes()
            .iter()
            .map(|node| {
                let first_child = child_indices.len();
                child_indices.extend_from_slice(node.children());
                GlueObject::from_node(node, first_child)
            })
            .collect::<Vec<_>>();
        let report = GlueStartupReport::new(
            bundle.resources().len(),
            bundle.actions().len(),
            objects.len(),
            objects
                .iter()
                .filter(|object| object.name().is_some())
                .count(),
            frames.state_count(),
            regions.state_count(),
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
            textures,
            objects,
            child_indices,
            report,
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

    /// Returns the archive-backed texture declaration plan.
    #[must_use]
    pub const fn textures(&self) -> &UiTexturePlan {
        &self.textures
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

    /// Returns mutable Lua/object state for event and frame dispatch.
    #[must_use]
    pub const fn runtime_mut(&mut self) -> &mut UiScriptRuntime {
        &mut self.runtime
    }
}
