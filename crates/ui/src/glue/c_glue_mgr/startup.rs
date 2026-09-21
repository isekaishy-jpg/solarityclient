//! Initial retained object construction, separate from live UI updates.

use solarity_asset::{AssetStore, AssetStoreHandle};

use super::{GlueManager, build_live_hierarchy, synchronize_resolved_dimensions, transaction};
use crate::glue::pointer::UiPointerPlan;
use crate::glue::{GlueError, GlueStartupReport};
use crate::startup::{StartupBudget, StartupTask};
use crate::{
    FontCatalog, UiAnimationPlan, UiBackdropPlan, UiBackdropStatePlan, UiBundle, UiEventArgument,
    UiEventPayload, UiFramePlan, UiGlyphAtlasPlan, UiLayoutPlan, UiManifestKind, UiObjectCatalog,
    UiObjectTree, UiPresentationPlan, UiRegionGeometryPlan, UiRegionStatePlan, UiRenderPlan,
    UiRuntimeTemplatePlan, UiScriptEnvironment, UiScriptPlan, UiScriptRuntime, UiScriptRuntimePlan,
    UiScrollFramePlan, UiTexturePlan, UiTextureStatePlan,
};

/// Immutable Glue declarations and character choices prepared without live Lua or RNG ownership.
pub struct GlueUiSources {
    character_creation: crate::glue::character::UiCharacterCreationCatalog,
    declarations: crate::xml::UiSourceImage,
    streaming_trial: bool,
}

impl GlueUiSources {
    /// Loads creation metadata, then validates GlueXML in authored source order.
    /// The temporary Lua validator is retired on the preparing thread.
    /// # Errors
    /// Returns the original catalog, archive, XML or Lua compilation failure.
    pub fn load(assets: &mut AssetStore, streaming_trial: bool) -> Result<Self, GlueError> {
        let character_creation =
            crate::glue::character::UiCharacterCreationCatalog::load(assets, streaming_trial)?;
        let (declarations, _validator) =
            crate::xml::UiSourceImage::load(assets, UiManifestKind::Glue)?;
        Ok(Self {
            character_creation,
            declarations,
            streaming_trial,
        })
    }
}

impl GlueManager {
    /// Publishes prepared sources into main-owned Lua, random and UI state.
    /// # Errors
    /// Returns the original environment, callback, layout or font failure.
    #[allow(clippy::too_many_arguments)]
    pub fn start_shared_with_sources(
        assets: AssetStoreHandle,
        logical_extent: (u32, u32),
        initial_screen: crate::GlueInitialScreen,
        cvar_values: &[(String, String)],
        addon_catalog: &crate::AddonCatalog,
        random: std::rc::Rc<std::cell::RefCell<solarity_cpu::BlizzardRand>>,
        sources: GlueUiSources,
        font_system: crate::FontSystem,
    ) -> Result<Self, GlueError> {
        let creation =
            crate::UiCharacterCreationState::from_catalog(sources.character_creation, random);
        let environment =
            UiScriptEnvironment::new(logical_extent.0, logical_extent.1, sources.streaming_trial)?
                .with_shared_asset_store(assets.clone())
                .with_font_system(font_system)
                .with_cvar_values(cvar_values)
                .with_addon_load_state(crate::UiAddonLoadState::from_catalog(addon_catalog))
                .with_character_creation_state(creation);
        Self::start_prepared_owner(
            assets,
            environment,
            UiManifestKind::Glue,
            Some(initial_screen),
            UiBundle::from_prepared_sources(sources.declarations),
        )
    }

    /// Constructs one independently owned active-world FrameXML runtime.
    pub(in crate::glue) async fn start_shared_frame(
        assets: AssetStoreHandle,
        environment: UiScriptEnvironment,
        bundle: UiBundle,
        budget: StartupBudget,
    ) -> Result<Self, GlueError> {
        Self::start_with_bundle(
            assets,
            environment,
            UiManifestKind::Frame,
            None,
            bundle,
            budget,
        )
        .await
    }

    /// Builds common retained script, object, layout, and rendering state.
    pub(super) fn start_shared_owner(
        assets: AssetStoreHandle,
        environment: UiScriptEnvironment,
        manifest_kind: UiManifestKind,
        initial_screen: Option<crate::glue::GlueInitialScreen>,
    ) -> Result<Self, GlueError> {
        let bundle = UiBundle::load(&mut assets.borrow_mut(), manifest_kind)?;
        Self::start_prepared_owner(assets, environment, manifest_kind, initial_screen, bundle)
    }

    fn start_prepared_owner(
        assets: AssetStoreHandle,
        environment: UiScriptEnvironment,
        manifest_kind: UiManifestKind,
        initial_screen: Option<crate::glue::GlueInitialScreen>,
        bundle: UiBundle,
    ) -> Result<Self, GlueError> {
        let _profile = solarity_profiling::profile!("ui.startup");
        StartupTask::new(move |budget| {
            Self::start_with_bundle(
                assets,
                environment,
                manifest_kind,
                initial_screen,
                bundle,
                budget,
            )
        })
        .complete()
    }

    /// Executes prepared declarations and creates one native presentation owner.
    async fn start_with_bundle(
        assets: AssetStoreHandle,
        environment: UiScriptEnvironment,
        manifest_kind: UiManifestKind,
        initial_screen: Option<crate::glue::GlueInitialScreen>,
        bundle: UiBundle,
        budget: StartupBudget,
    ) -> Result<Self, GlueError> {
        if manifest_kind == UiManifestKind::Glue {
            environment.record_model_actions();
        }
        let logical_extent = environment.logical_extent();
        let fonts = {
            let _profile = solarity_profiling::profile!("ui.startup.fonts");
            FontCatalog::from_bundle(&bundle)?
        };
        budget.checkpoint().await;
        let catalog = {
            let _profile = solarity_profiling::profile!("ui.startup.catalog");
            UiObjectCatalog::from_bundle(&bundle, &fonts)?
        };
        budget.checkpoint().await;
        let tree = {
            let _profile = solarity_profiling::profile!("ui.startup.object_tree");
            UiObjectTree::from_catalog(&catalog, &fonts)?
        };
        budget.checkpoint().await;
        let frame_plan = UiFramePlan::from_tree(&tree)?;
        let frames = frame_plan.resolve(&tree)?;
        let layout = UiLayoutPlan::from_tree(&tree)?;
        let regions = {
            let _profile = solarity_profiling::profile!("ui.startup.regions");
            UiRegionStatePlan::resolve(&tree, &layout)?
        };
        budget.checkpoint().await;
        let scripts = UiScriptPlan::from_tree(&tree, bundle.lua())?;
        let templates = {
            let _profile = solarity_profiling::profile!("ui.startup.templates");
            UiRuntimeTemplatePlan::from_catalog(&catalog, &fonts, bundle.lua())?
        };
        budget.checkpoint().await;
        let textures = UiTexturePlan::from_tree(&tree)?;
        let texture_states = UiTextureStatePlan::resolve(&tree, &textures)?;
        let backdrop_plan = UiBackdropPlan::from_tree(&tree)?;
        let backdrops = UiBackdropStatePlan::resolve(&tree, &backdrop_plan)?;
        let animations = {
            let _profile = solarity_profiling::profile!("ui.startup.animations");
            UiAnimationPlan::from_tree(&tree)?
        };
        budget.checkpoint().await;
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
        let mut runtime = {
            let _profile = solarity_profiling::profile!("ui.startup.runtime");
            UiScriptRuntime::new(&bundle, &runtime_plan, environment.clone())?
        };
        budget.checkpoint().await;
        while runtime.execute_next(&bundle, &tree, &scripts)? {
            budget.checkpoint().await;
        }
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

        budget.checkpoint().await;
        let mut live = runtime
            .snapshot_objects_for_startup(&bundle, &budget)
            .await?;
        budget.checkpoint().await;
        let geometry = {
            let _profile = solarity_profiling::profile!("ui.startup.geometry");
            runtime.font_system().prepare(
                &mut assets.borrow_mut(),
                &mut live,
                move |live, _, _| UiRegionGeometryPlan::resolve(live, ui_extent),
            )??
        };
        budget.checkpoint().await;
        runtime.publish_resolved_geometry(&bundle, &geometry)?;
        let mut native = super::preparation::UiNativeState {
            fonts,
            live,
            geometry,
            backdrops,
            ..Default::default()
        };
        let html = runtime.simple_html().clone();
        runtime.font_system().prepare(
            &mut assets.borrow_mut(),
            &mut native,
            move |state, fonts, assets| {
                synchronize_resolved_dimensions(&mut state.live, &state.geometry);
                state.scroll_frames = UiScrollFramePlan::from_live(&state.live);
                state.glyphs = UiGlyphAtlasPlan::from_live_ui(
                    &html,
                    &state.live,
                    &state.geometry,
                    &state.fonts,
                    assets,
                    logical_extent.1,
                    fonts.clone(),
                )?;
                state.presentation =
                    UiPresentationPlan::resolve(&state.live, &state.geometry, &state.backdrops);
                state.render_plan = UiRenderPlan::prepare_with_glyphs(
                    &state.presentation,
                    &state.glyphs,
                    &state.geometry,
                    &state.scroll_frames,
                    ui_extent,
                )?;
                (state.objects, state.child_indices) = build_live_hierarchy(&state.live)?;
                state.pointer = UiPointerPlan::from_live(&state.live);
                Ok::<_, GlueError>(())
            },
        )??;
        budget.checkpoint().await;
        let report = GlueStartupReport::new(
            bundle.resources().len(),
            bundle.actions().len(),
            native.objects.len(),
            native
                .objects
                .iter()
                .filter(|object| object.name().is_some())
                .count(),
            frames.state_count(),
            native.geometry.region_count(),
            textures.layer_count(),
            runtime.executed_chunk_count(),
            runtime.executed_load_handler_count(),
        );

        Ok(Self {
            runtime,
            native,
            scripts,
            templates,
            frames,
            regions,
            textures,
            texture_states,
            pointer_capture: None,
            edit_box_pointer_anchor: None,
            pointer_hover: None,
            deferred_scroll_refresh: Vec::new(),
            deferred_presentation: transaction::DeferredPresentation::default(),
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
