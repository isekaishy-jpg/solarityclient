//! Initial retained object construction, separate from live UI updates.

use solarity_asset::AssetStoreHandle;

use super::{GlueManager, build_live_hierarchy, synchronize_resolved_dimensions, transaction};
use crate::glue::pointer::UiPointerPlan;
use crate::glue::{GlueError, GlueStartupReport};
use crate::{
    FontCatalog, UiAnimationPlan, UiBackdropPlan, UiBackdropStatePlan, UiBundle, UiEventArgument,
    UiEventPayload, UiFramePlan, UiGlyphAtlasPlan, UiLayoutPlan, UiManifestKind, UiObjectCatalog,
    UiObjectTree, UiPresentationPlan, UiRegionGeometryPlan, UiRegionStatePlan, UiRenderPlan,
    UiRuntimeTemplatePlan, UiScriptEnvironment, UiScriptPlan, UiScriptRuntime, UiScriptRuntimePlan,
    UiScrollFramePlan, UiTexturePlan, UiTextureStatePlan,
};

impl GlueManager {
    /// Constructs one independently owned active-world FrameXML runtime.
    pub(in crate::glue) fn start_shared_frame(
        assets: AssetStoreHandle,
        environment: UiScriptEnvironment,
        bundle: UiBundle,
    ) -> Result<Self, GlueError> {
        Self::start_with_bundle(assets, environment, UiManifestKind::Frame, None, bundle)
    }

    /// Builds common retained script, object, layout, and rendering state.
    pub(super) fn start_shared_owner(
        assets: AssetStoreHandle,
        environment: UiScriptEnvironment,
        manifest_kind: UiManifestKind,
        initial_screen: Option<crate::glue::GlueInitialScreen>,
    ) -> Result<Self, GlueError> {
        let bundle = UiBundle::load(&mut assets.borrow_mut(), manifest_kind)?;
        Self::start_with_bundle(assets, environment, manifest_kind, initial_screen, bundle)
    }

    /// Executes prepared declarations and creates one native presentation owner.
    fn start_with_bundle(
        assets: AssetStoreHandle,
        environment: UiScriptEnvironment,
        manifest_kind: UiManifestKind,
        initial_screen: Option<crate::glue::GlueInitialScreen>,
        bundle: UiBundle,
    ) -> Result<Self, GlueError> {
        if manifest_kind == UiManifestKind::Glue {
            environment.record_model_actions();
        }
        let mut ui_profile = solarity_profiling::profile!("ui.startup");
        let logical_extent = environment.logical_extent();
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
        ui_profile.mark("static plans");
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
        ui_profile.mark("Lua runtime");
        runtime.execute_all(&bundle, &tree, &scripts)?;
        if manifest_kind == UiManifestKind::Frame {
            runtime.initialize_frame_scale(&bundle, &environment)?;
        }
        if let Some(initial_screen) = initial_screen {
            let _dispatch =
                runtime.dispatch_glue_event(&bundle, "FRAMES_LOADED", &UiEventPayload::empty())?;
            let initial_payload = UiEventPayload::new([UiEventArgument::String(
                initial_screen.script_name().to_owned(),
            )]);
            let _dispatch =
                runtime.dispatch_glue_event(&bundle, "SET_GLUE_SCREEN", &initial_payload)?;
        }
        ui_profile.mark("Lua execution");

        let mut live = runtime.snapshot_objects(&bundle)?;
        let geometry = UiRegionGeometryPlan::resolve(&live, ui_extent)?;
        runtime.publish_resolved_geometry(&bundle, &geometry)?;
        synchronize_resolved_dimensions(&mut live, &geometry);
        let scroll_frames = UiScrollFramePlan::from_live(&live);
        let glyphs = UiGlyphAtlasPlan::from_live_ui(
            runtime.simple_html(),
            &live,
            &geometry,
            &fonts,
            &mut assets.borrow_mut(),
            logical_extent.1,
            runtime.font_system(),
        )?;
        let presentation = UiPresentationPlan::resolve(&live, &geometry, &backdrops);
        let render_plan = UiRenderPlan::prepare_with_glyphs(
            &presentation,
            &glyphs,
            &geometry,
            &scroll_frames,
            ui_extent,
        )?;
        ui_profile.mark("live presentation");
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
            live,
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
            edit_box_pointer_anchor: None,
            pointer_hover: None,
            deferred_scroll_refresh: Vec::new(),
            deferred_presentation: transaction::DeferredPresentation::default(),
            visual_indices: Vec::new(),
            visual_work: Vec::new(),
            incremental_visual_updates: std::env::var_os("SOLARITY_UI_FULL_VISUAL_REFRESH")
                .is_none(),
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
}
