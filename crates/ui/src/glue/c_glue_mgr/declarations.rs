//! Owned declaration products prepared before entering the live Lua owner.

use crate::glue::GlueError;
use crate::script::{PreparedRuntimeTemplatePlan, PreparedScriptPlan, prepared::ExportFunctions};
use crate::{
    FontCatalog, FontSystem, UiAnimationPlan, UiBackdropPlan, UiBackdropStatePlan, UiBundle,
    UiFramePlan, UiFrameStatePlan, UiLayoutPlan, UiObjectCatalog, UiObjectTree, UiRegionStatePlan,
    UiRuntimeTemplatePlan, UiScriptPlan, UiSimpleHtmlError, UiSimpleHtmlPlan, UiTexturePlan,
    UiTextureStatePlan,
};
use solarity_asset::AssetStore;

pub(super) struct PreparedDeclarations {
    pub fonts: FontCatalog,
    pub tree: UiObjectTree,
    pub frames: UiFrameStatePlan,
    pub regions: UiRegionStatePlan,
    pub scripts: PreparedScriptPlan,
    pub templates: PreparedRuntimeTemplatePlan,
    pub textures: UiTexturePlan,
    pub texture_states: UiTextureStatePlan,
    pub backdrops: UiBackdropStatePlan,
    pub animations: UiAnimationPlan,
    // Consume at the original runtime initialization boundary, after Lua setup.
    pub simple_html: Result<UiSimpleHtmlPlan, UiSimpleHtmlError>,
}

impl PreparedDeclarations {
    pub(super) fn load(
        bundle: &UiBundle,
        fonts_system: &mut FontSystem,
        assets: &mut AssetStore,
        logical_height: u32,
    ) -> Result<Self, GlueError> {
        let _preparation = solarity_profiling::profile!("ui.startup.declarations");
        let fonts = {
            let _profile = solarity_profiling::profile!("ui.startup.fonts");
            FontCatalog::from_bundle(bundle)?
        };
        let catalog = {
            let _profile = solarity_profiling::profile!("ui.startup.catalog");
            UiObjectCatalog::from_bundle(bundle, &fonts)?
        };
        let tree = {
            let _profile = solarity_profiling::profile!("ui.startup.object_tree");
            UiObjectTree::from_catalog(&catalog, &fonts)?
        };
        let frames = UiFramePlan::from_tree(&tree)?.resolve(&tree)?;
        let layout = UiLayoutPlan::from_tree(&tree)?;
        let regions = {
            let _profile = solarity_profiling::profile!("ui.startup.regions");
            UiRegionStatePlan::resolve(&tree, &layout)?
        };
        let scripts = UiScriptPlan::from_tree(&tree, bundle.lua())?;
        let templates = {
            let _profile = solarity_profiling::profile!("ui.startup.templates");
            UiRuntimeTemplatePlan::from_catalog(&catalog, &fonts, bundle.lua())?
        };
        let textures = UiTexturePlan::from_tree(&tree)?;
        let texture_states = UiTextureStatePlan::resolve(&tree, &textures)?;
        let backdrops = UiBackdropStatePlan::resolve(&tree, &UiBackdropPlan::from_tree(&tree)?)?;
        let animations = {
            let _profile = solarity_profiling::profile!("ui.startup.animations");
            UiAnimationPlan::from_tree(&tree)?
        };
        let simple_html = UiSimpleHtmlPlan::resolve_with_system(
            &tree,
            &regions,
            &fonts,
            assets,
            logical_height,
            fonts_system.clone(),
        );
        let mut functions = ExportFunctions::default();
        let scripts = scripts.into_prepared(bundle.lua(), &mut functions)?;
        let templates = templates.into_prepared(bundle.lua(), &mut functions)?;
        Ok(Self {
            fonts,
            tree,
            frames,
            regions,
            scripts,
            templates,
            textures,
            texture_states,
            backdrops,
            animations,
            simple_html,
        })
    }
}
