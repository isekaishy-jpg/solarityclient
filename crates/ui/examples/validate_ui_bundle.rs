//! Validates one built-in UI bundle against an installed stock client.

use std::collections::BTreeSet;
use std::error::Error;
use std::io::{Error as IoError, ErrorKind};
use std::path::PathBuf;

use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, BlpTextureCache, ClientDataRoot, Locale,
};
use solarity_ui::{
    FontCatalog, FontRasterization, FontSystem, GlueManager, UiAnimationPlan, UiBindingAssignments,
    UiBindingCatalog, UiBundle, UiFactionGroup, UiFramePlan, UiLayoutPlan, UiManifestKind,
    UiObjectCatalog, UiObjectTree, UiPlayerClassState, UiPlayerFactionState, UiPlayerIdentityState,
    UiPlayerLanguage, UiPlayerProgressionState, UiPlayerRaceState, UiPlayerState,
    UiPlayerVitalsState, UiRealmDate, UiRealmTime, UiRegionStatePlan, UiResourceContent,
    UiRuntimeTemplatePlan, UiScriptEnvironment, UiScriptPlan, UiScriptRuntime, UiScriptRuntimePlan,
    UiSpellBookTab, UiTextureFile, UiTexturePlan, UiTextureStatePlan, UiUnitPowerType, UiZoneState,
};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os();
    let _executable = arguments.next();
    let data_root = arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| argument_error("missing client Data directory"))?;
    let locale = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(|| argument_error("missing ASCII locale such as enUS"))?
        .parse::<Locale>()?;
    let kind = match arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .as_deref()
    {
        Some("glue") => UiManifestKind::Glue,
        Some("frame") => UiManifestKind::Frame,
        _ => return Err(argument_error("bundle must be glue or frame").into()),
    };
    let execution_environment = match arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .as_deref()
    {
        None => None,
        Some("execute") => {
            let width = parse_dimension(arguments.next(), "logical width")?;
            let height = parse_dimension(arguments.next(), "logical height")?;
            let environment = UiScriptEnvironment::new(width, height, false)?;
            if kind == UiManifestKind::Frame {
                let money_copper = parse_player_money(arguments.next())?;
                let player_xp = parse_player_experience(arguments.next(), "player XP")?;
                let next_level_xp =
                    parse_player_experience(arguments.next(), "player next-level XP")?;
                let world = environment.world_state();
                world.enter_player(UiPlayerState::new(money_copper));
                world.set_player_identity(UiPlayerIdentityState::new("SolarityTester", 80));
                world.set_player_class(UiPlayerClassState::new("Warrior", "WARRIOR", 1));
                world.set_player_race(UiPlayerRaceState::new("Human", "Human", 1));
                world.set_player_progression(UiPlayerProgressionState::new(
                    player_xp,
                    next_level_xp,
                ));
                world.set_player_vitals(UiPlayerVitalsState::new(
                    100,
                    100,
                    100,
                    100,
                    UiUnitPowerType::Mana,
                ));
                // This offline executor uses one explicit, internally
                // consistent player identity fixture. Runtime composition
                // publishes faction from the selected character's race row.
                world.set_player_faction(UiPlayerFactionState::new(
                    UiFactionGroup::Alliance,
                    "Alliance",
                ));
                // Common is language row 7 for the explicit Alliance fixture.
                world.set_player_default_language(UiPlayerLanguage::new(7, "Common"));
                // Empty labels and no PvP classification are an explicit
                // pre-map update, matching the temporal state before the
                // world service publishes its first area transition.
                world.set_zone(UiZoneState::new("", "", "", "", None, false, None));
                // The offline path supplies an explicit server-time fixture;
                // UI code never substitutes the host wall clock for realm time.
                // Tuesday, December 8, 2009 is internally consistent with the
                // stock 3.3 calendar era and uses Lua's one-based weekday.
                world.set_realm_date(UiRealmDate::new(3, 12, 8, 2009)?);
                world.set_realm_time(UiRealmTime::new(12, 0)?);
                // This explicit empty image represents a character whose
                // authoritative 144-slot server packet contains no actions.
                environment.action_bar_state().set_slots([0; 144]);
                // FrameXML is only entered with a character spell book. This
                // explicit zero-spell General tab keeps the offline fixture
                // temporally valid without inventing learned Warrior spells.
                environment
                    .spell_book_state()
                    .set_tabs(vec![UiSpellBookTab::new(
                        "General",
                        "Interface\\Icons\\INV_Misc_QuestionMark",
                        0,
                        0,
                    )]);
            }
            Some(environment)
        }
        Some(_) => return Err(argument_error("optional mode must be execute").into()),
    };
    if arguments.next().is_some() {
        return Err(argument_error("unexpected extra arguments").into());
    }

    let root = ClientDataRoot::new(data_root)?;
    let catalog = ArchiveCatalog::discover(root, locale)?;
    let archive_count = catalog.descriptors().len();
    let execution_environment = match execution_environment {
        Some(mut environment) => {
            let mut environment_assets = AssetStore::mount(catalog.clone())?;
            if kind == UiManifestKind::Frame {
                let binding_catalog = UiBindingCatalog::load_builtin(&mut environment_assets)?;
                let bindings =
                    UiBindingAssignments::load_defaults(&mut environment_assets, &binding_catalog)?;
                environment = environment.with_binding_assignments(bindings);
            }
            Some(environment.with_asset_store(environment_assets))
        }
        None => None,
    };
    let mut store = AssetStore::mount(catalog.clone())?;
    let bundle = UiBundle::load(&mut store, kind)?;
    let xml_count = bundle
        .resources()
        .iter()
        .filter(|resource| matches!(resource.content(), UiResourceContent::Xml(_)))
        .count();
    let lua_count = bundle.resources().len() - xml_count;
    let font_catalog = FontCatalog::from_bundle(&bundle)?;
    let object_catalog = UiObjectCatalog::from_bundle(&bundle, &font_catalog)?;
    let object_tree = UiObjectTree::from_catalog(&object_catalog, &font_catalog)?;
    let scheduled_object_count = object_tree
        .batches()
        .iter()
        .map(|batch| batch.node_count())
        .sum::<usize>();
    if scheduled_object_count != object_tree.nodes().len() {
        return Err(IoError::new(
            ErrorKind::InvalidData,
            format!(
                "construction batches cover {scheduled_object_count} of {} objects",
                object_tree.nodes().len()
            ),
        )
        .into());
    }
    let frame_plan = UiFramePlan::from_tree(&object_tree)?;
    let frame_states = frame_plan.resolve(&object_tree)?;
    let layout_plan = UiLayoutPlan::from_tree(&object_tree)?;
    let region_states = UiRegionStatePlan::resolve(&object_tree, &layout_plan)?;
    let script_plan = UiScriptPlan::from_tree(&object_tree, bundle.lua())?;
    let runtime_templates =
        UiRuntimeTemplatePlan::from_catalog(&object_catalog, &font_catalog, bundle.lua())?;
    let texture_plan = UiTexturePlan::from_tree(&object_tree)?;
    let texture_states = UiTextureStatePlan::resolve(&object_tree, &texture_plan)?;
    let animation_plan = UiAnimationPlan::from_tree(&object_tree)?;
    let script_runtime_plan = UiScriptRuntimePlan::new(
        &object_tree,
        &animation_plan,
        &frame_states,
        &region_states,
        &runtime_templates,
        &font_catalog,
        &texture_states,
    );
    let manager_extent = execution_environment
        .as_ref()
        .map(UiScriptEnvironment::logical_extent);
    if let Some(environment) = execution_environment {
        let mut script_runtime = UiScriptRuntime::new(&bundle, &script_runtime_plan, environment)?;
        script_runtime.execute_all(&bundle, &object_tree, &script_plan)?;
        println!(
            "executed {} actions, registered {} objects, ran {} Lua chunks and {} OnLoad handlers",
            script_runtime.next_action(),
            script_runtime.registered_object_count(),
            script_runtime.executed_chunk_count(),
            script_runtime.executed_load_handler_count()
        );
    }
    if kind == UiManifestKind::Glue
        && let Some(extent) = manager_extent
    {
        let manager = GlueManager::start(AssetStore::mount(catalog)?, extent, false)?;
        let mut texture_cache = BlpTextureCache::new();
        let texture_bindings = manager.load_blocking_render_textures(&mut texture_cache)?;
        println!(
            "activated stock login lifecycle with {} presentation packets, {} texture members, {} renderer batches, {} resident textures, and {} pending textures",
            manager.presentation().packets().len(),
            manager.presentation().member_count(),
            manager.render_plan().mesh().batches().len(),
            texture_bindings.resident_count(),
            texture_bindings.pending_count()
        );
    }
    let texture_paths = object_tree
        .nodes()
        .iter()
        .enumerate()
        .filter_map(|(index, _)| texture_plan.node(index))
        .flat_map(|node| texture_plan.layers_for(node))
        .filter_map(|layer| match layer.file() {
            Some(UiTextureFile::Asset(path)) => Some(path.clone()),
            Some(UiTextureFile::Dynamic) | None => None,
        })
        .collect::<BTreeSet<_>>();
    for path in &texture_paths {
        let _asset = store.read(path)?;
    }
    let named_object_count = object_tree
        .nodes()
        .iter()
        .filter(|node| node.name().is_some())
        .count();
    let draw_layer_count = object_tree
        .nodes()
        .iter()
        .flat_map(|node| node.layers())
        .filter(|layer| layer.draw_layer().is_some())
        .count();
    let font_path = AssetPath::new("Fonts\\FRIZQT__.TTF")?;
    let mut fonts = FontSystem::new()?;
    let glyph = fonts.rasterize(
        &mut store,
        &font_path,
        16,
        'A',
        FontRasterization::Antialiased,
    )?;

    println!(
        "validated {:?}: {archive_count} archives, {} resources ({xml_count} XML, {lua_count} Lua), {} ordered actions, {} fonts, {} templates expanding to {} runtime prototype nodes, {} live roots in {} construction batches, {} instantiated objects ({named_object_count} named, {} top-level), {} resolved frames from {} property layers, {draw_layer_count} layered declarations, {} layout layers and {} authored anchors resolving to {} region states and {} final anchors, {} script declarations resolving to {} active bindings and {} unique inline functions, {} texture layers referencing {} unique archive assets, FRIZQT__ 'A' {}x{}",
        bundle.manifest().kind(),
        bundle.resources().len(),
        bundle.actions().len(),
        font_catalog.definitions().len(),
        object_catalog.templates().len(),
        runtime_templates.node_count(),
        object_catalog.roots().len(),
        object_tree.batches().len(),
        object_tree.nodes().len(),
        object_tree.top_level().len(),
        frame_states.state_count(),
        frame_plan.layer_count(),
        layout_plan.layer_count(),
        layout_plan.anchor_count(),
        region_states.state_count(),
        region_states.anchor_count(),
        script_plan.declaration_count(),
        script_plan.binding_count(),
        script_plan.function_count(),
        texture_plan.layer_count(),
        texture_paths.len(),
        glyph.width(),
        glyph.height()
    );
    Ok(())
}

fn argument_error(message: &str) -> IoError {
    IoError::new(
        ErrorKind::InvalidInput,
        format!(
            "{message}; usage: validate_ui_bundle <Data> <locale> <glue|frame> [execute <logical-width> <logical-height> [frame-player-money-copper frame-player-xp frame-next-level-xp]]"
        ),
    )
}

fn parse_player_money(value: Option<std::ffi::OsString>) -> Result<u32, IoError> {
    value
        .and_then(|value| value.into_string().ok())
        .ok_or_else(|| argument_error("frame execution requires authoritative player money"))?
        .parse::<u32>()
        .map_err(|_| argument_error("invalid frame player money"))
}

fn parse_player_experience(value: Option<std::ffi::OsString>, label: &str) -> Result<u32, IoError> {
    value
        .and_then(|value| value.into_string().ok())
        .ok_or_else(|| argument_error(&format!("frame execution requires authoritative {label}")))?
        .parse::<u32>()
        .map_err(|_| argument_error(&format!("invalid frame {label}")))
}

fn parse_dimension(value: Option<std::ffi::OsString>, label: &str) -> Result<u32, IoError> {
    let value = value
        .and_then(|value| value.into_string().ok())
        .ok_or_else(|| argument_error(&format!("missing {label}")))?;
    let dimension = value
        .parse::<u32>()
        .map_err(|_| argument_error(&format!("invalid {label}")))?;
    if dimension == 0 {
        return Err(argument_error(&format!("{label} must be nonzero")));
    }
    Ok(dimension)
}
