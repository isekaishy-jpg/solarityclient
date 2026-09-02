//! Exercises real locale-backed first-run agreements and later-run bypass.

use std::cell::RefCell;
use std::error::Error;
use std::io::{Error as IoError, ErrorKind};
use std::path::PathBuf;
use std::rc::Rc;

use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, AssetStoreHandle, BlpTextureCache, ClientDataRoot,
    DecodedM2Model, Locale,
};
use solarity_cpu::BlizzardRand;
use solarity_ui::{
    AddonCatalog, GlueInitialScreen, GlueManager, UiCharacterDirectory, UiCharacterEquipment,
    UiCharacterExpansion, UiCharacterInfo, UiCharacterPetPreview, UiEventArgument, UiEventPayload,
    UiGlueNetworkAction, UiGlueNetworkStatus, UiKeyboardModifiers, UiObjectRole, UiPointerButton,
    UiTextureSource,
};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os();
    let _executable = arguments.next();
    let data_root = arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(usage_error)?;
    let locale = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(usage_error)?
        .parse::<Locale>()?;
    if arguments.next().is_some() {
        return Err(usage_error().into());
    }

    let catalog = ArchiveCatalog::discover(ClientDataRoot::new(data_root.clone())?, locale)?;
    let mut inspection_store = AssetStore::mount(catalog.clone())?;
    let addon_catalog = AddonCatalog::discover(&mut inspection_store)?;
    let login_model = DecodedM2Model::load_primary_profile(
        &mut inspection_store,
        &AssetPath::new(
            "Interface\\Glues\\Models\\UI_MainMenu_Northrend\\UI_MainMenu_Northrend.m2",
        )?,
    )?;
    println!(
        "login model: cameras={} sequences={} textures={} particles={} ribbons={}",
        login_model.animations().cameras().len(),
        login_model.animations().sequences().len(),
        login_model.textures().len(),
        login_model.animations().particles().len(),
        login_model.animations().ribbons().len()
    );
    for (index, camera) in login_model.animations().cameras().iter().enumerate() {
        println!(
            "login camera {index}: fov={} near={} far={} position={:?} target={:?}",
            camera.field_of_view_radians(),
            camera.near_clip(),
            camera.far_clip(),
            camera.position_base(),
            camera.target_position_base(),
        );
    }
    let mut manager = GlueManager::start_shared_with_profile(
        AssetStoreHandle::new(AssetStore::mount(catalog)?),
        (1280, 720),
        false,
        GlueInitialScreen::Login,
        &[
            ("readEULA".to_owned(), "-1".to_owned()),
            ("readTOS".to_owned(), "-1".to_owned()),
            ("readTerminationWithoutNotice".to_owned(), "1".to_owned()),
            ("readScanning".to_owned(), "1".to_owned()),
            ("readContest".to_owned(), "1".to_owned()),
        ],
        &addon_catalog,
    )?;

    accept_notice(&mut manager, "EULAScrollFrame", "TOSAccept", "readEULA")?;
    accept_notice(&mut manager, "TOSScrollFrame", "TOSAccept", "readTOS")?;
    let changes = manager.take_changed_cvars();
    if manager.current_screen() != "login"
        || !contains_change(&changes, "readEULA", "1")
        || !contains_change(&changes, "readTOS", "1")
    {
        return Err(invalid_data(format!(
            "first-run agreements did not reach login with persistent acceptance: screen={}, changes={changes:?}",
            manager.current_screen()
        ))
        .into());
    }
    drop(manager);

    let accepted_cvars = [
        ("readEULA".to_owned(), "1".to_owned()),
        ("readTOS".to_owned(), "1".to_owned()),
        ("readTerminationWithoutNotice".to_owned(), "1".to_owned()),
        ("readScanning".to_owned(), "1".to_owned()),
        ("readContest".to_owned(), "1".to_owned()),
    ];
    let catalog = ArchiveCatalog::discover(ClientDataRoot::new(data_root)?, locale)?;
    let mut later = GlueManager::start_shared_with_profile_and_random(
        AssetStoreHandle::new(AssetStore::mount(catalog)?),
        (1280, 720),
        false,
        GlueInitialScreen::Login,
        &accepted_cvars,
        &addon_catalog,
        Rc::new(RefCell::new(BlizzardRand::new(0x1234_5678))),
    )?;
    if later.current_screen() != "login"
        || !object_is_shown(&later, "AccountLoginUI")?
        || object_is_shown(&later, "TOSFrame")?
        || later.media_intent().movie().is_some()
    {
        return Err(invalid_data(
            "accepted later-run state did not bypass MOV and legal notices".to_owned(),
        )
        .into());
    }
    later.update(1.5)?;
    if later.current_screen() != "login" {
        return Err(invalid_data(
            "authored login fade-in changed the active Glue screen".to_owned(),
        )
        .into());
    }
    validate_login_presentation(&later)?;
    let mut texture_cache = BlpTextureCache::new();
    let texture_bindings = later.load_blocking_render_textures(&mut texture_cache)?;
    if texture_bindings.pending_count() != 0 {
        return Err(
            invalid_data("stock login left non-blocking textures pending".to_owned()).into(),
        );
    }
    report_login_presentation(&later, texture_bindings.resident_count());
    validate_login_input(&mut later)?;
    let atlas_extent = later.glyphs().extent();
    let atlas_bytes = later.glyphs().rgba8().len();
    let visible_glyphs = visible_glyph_owners(&later).len();
    validate_initial_empty_character_selection(&mut later)?;
    validate_character_selection(&mut later)?;
    validate_empty_character_selection(&mut later)?;
    validate_character_creation(&mut later)?;
    println!(
        "validated first-run agreements, later-run bypass, authored login input, character selection, and character creation: changes={changes:?} atlas={atlas_extent:?} atlas_bytes={atlas_bytes} login_visible_glyphs={visible_glyphs}"
    );
    Ok(())
}

fn validate_initial_empty_character_selection(
    manager: &mut GlueManager,
) -> Result<(), Box<dyn Error>> {
    manager.set_network_status(UiGlueNetworkStatus::new(
        Some("Validation Realm".to_owned()),
        true,
    ));
    manager.set_character_directory(UiCharacterDirectory::new(Vec::new(), "Orc".to_owned()));
    manager.dispatch_event(
        "SET_GLUE_SCREEN",
        &UiEventPayload::new([UiEventArgument::String("charselect".to_owned())])?,
    )?;
    manager.dispatch_event(
        "CHARACTER_LIST_UPDATE",
        &UiEventPayload::new([UiEventArgument::Integer(0)])?,
    )?;
    manager.update(0.25)?;
    manager.update(0.25)?;

    let create = object_index(manager, "CharSelectCreateCharacterButton")?;
    let create_geometry = manager
        .geometry()
        .region(create)
        .ok_or_else(|| invalid_data("empty-first create button has no geometry".to_owned()))?;
    let normal_texture = manager
        .objects()
        .iter()
        .enumerate()
        .find(|(_, object)| {
            object.parent() == Some(create) && object.role() == UiObjectRole::NormalTexture
        })
        .map(|(index, _)| index)
        .ok_or_else(|| invalid_data("empty-first create button has no NormalTexture".to_owned()))?;
    let button_text = manager
        .objects()
        .iter()
        .enumerate()
        .find(|(_, object)| {
            object.parent() == Some(create) && object.role() == UiObjectRole::ButtonText
        })
        .map(|(index, _)| index)
        .ok_or_else(|| invalid_data("empty-first create button has no ButtonText".to_owned()))?;
    let mesh_objects = manager.render_plan().mesh().object_indices();
    let create_in_mesh = manager
        .presentation()
        .members_in_draw_order()
        .iter()
        .any(|quad| quad.object_index() == normal_texture)
        && mesh_objects.contains(&normal_texture)
        && visible_glyph_owners(manager).contains(&button_text)
        && mesh_objects.contains(&button_text);
    if manager.current_screen() != "charselect"
        || !create_geometry.effectively_shown()
        || !create_in_mesh
    {
        return Err(invalid_data(format!(
            "empty-first selection omitted its create control: screen={} shown={} mesh={create_in_mesh}",
            manager.current_screen(),
            create_geometry.effectively_shown()
        ))
        .into());
    }
    println!(
        "empty-first character selection: create_bounds={:?} quads={} batches={}",
        create_geometry.presentation_bounds(),
        manager.presentation().member_count(),
        manager.render_plan().mesh().batches().len(),
    );
    manager.dispatch_event(
        "SET_GLUE_SCREEN",
        &UiEventPayload::new([UiEventArgument::String("login".to_owned())])?,
    )?;
    manager.update(0.25)?;
    manager.update(0.25)?;
    if manager.current_screen() != "login" {
        return Err(invalid_data("empty-first validation did not restore login".to_owned()).into());
    }
    while manager.take_network_action().is_some() {}
    Ok(())
}

fn validate_character_creation(manager: &mut GlueManager) -> Result<(), Box<dyn Error>> {
    manager.set_character_creation_expansion(UiCharacterExpansion::WRATH_OF_THE_LICH_KING);
    if manager.current_screen() != "charcreate" {
        manager.dispatch_event(
            "SET_GLUE_SCREEN",
            &UiEventPayload::new([UiEventArgument::String("charcreate".to_owned())])?,
        )?;
        manager.update(0.25)?;
        manager.update(0.25)?;
    }
    if manager.current_screen() != "charcreate" || !object_is_shown(manager, "CharacterCreate")? {
        return Err(invalid_data(format!(
            "stock character creation did not replace selection: screen={}",
            manager.current_screen()
        ))
        .into());
    }
    let globals = manager.bundle().lua().globals();
    manager
        .bundle()
        .lua()
        .load(
            "SOLARITY_RACE_VALUE_COUNT = select('#', GetAvailableRaces()); SOLARITY_CLASS_VALUE_COUNT = select('#', GetAvailableClasses())",
        )
        .exec()?;
    let race_value_count = globals.get::<usize>("SOLARITY_RACE_VALUE_COUNT")?;
    let class_value_count = globals.get::<usize>("SOLARITY_CLASS_VALUE_COUNT")?;
    if race_value_count != 30 || class_value_count != 30 {
        return Err(invalid_data(format!(
            "creation metadata returned {} race values and {} class values",
            race_value_count, class_value_count
        ))
        .into());
    }
    let background = globals
        .get::<mlua::Function>("GetCreateBackgroundModel")?
        .call::<String>(())?;
    if background.is_empty() {
        return Err(invalid_data("creation selected no stock background".to_owned()).into());
    }
    globals
        .get::<mlua::Function>("RandomizeCharCustomization")?
        .call::<()>(())?;
    globals
        .get::<mlua::Function>("CreateCharacter")?
        .call::<()>("Validationhero")?;
    let Some(UiGlueNetworkAction::CreateCharacter(request)) = manager.take_network_action() else {
        return Err(invalid_data("CreateCharacter emitted no protocol request".to_owned()).into());
    };
    if request.name() != "Validationhero"
        || request.race_id() == 0
        || request.class_id() == 0
        || request.gender_id() > 1
    {
        return Err(invalid_data(format!(
            "creation request has invalid identity fields: {request:?}"
        ))
        .into());
    }
    println!(
        "character creation: background={background} race={} class={} gender={} appearance={:?}",
        request.race_id(),
        request.class_id(),
        request.gender_id(),
        request.appearance()
    );
    Ok(())
}

fn validate_empty_character_selection(manager: &mut GlueManager) -> Result<(), Box<dyn Error>> {
    manager.set_character_directory(UiCharacterDirectory::new(Vec::new(), "Orc".to_owned()));
    manager.dispatch_event(
        "CHARACTER_LIST_UPDATE",
        &UiEventPayload::new([UiEventArgument::Integer(0)])?,
    )?;

    let model = manager
        .presentation()
        .models()
        .iter()
        .find(|model| manager.objects()[model.object_index()].name() == Some("CharacterSelect"))
        .ok_or_else(|| invalid_data("empty character selection has no ModelFFX".to_owned()))?;
    let model_path = model.path().as_str().to_owned();
    if model_path != "INTERFACE\\GLUES\\MODELS\\UI_ORC\\UI_ORC.M2" {
        return Err(invalid_data(format!(
            "empty character selection did not use stock's Orc default: {}",
            model_path
        ))
        .into());
    }

    let create = object_index(manager, "CharSelectCreateCharacterButton")?;
    let create_bounds = manager
        .geometry()
        .region(create)
        .ok_or_else(|| invalid_data("create-character button has no geometry".to_owned()))?;
    let create_presentation_bounds = create_bounds.presentation_bounds();
    if !create_bounds.effectively_shown()
        || (create_presentation_bounds.height() - 45.0).abs() > 0.000_01
    {
        return Err(invalid_data(format!(
            "empty account did not show the stock 45px create-character button: {create_bounds:?}"
        ))
        .into());
    }

    let normal_texture = manager
        .objects()
        .iter()
        .enumerate()
        .find(|(_, object)| {
            object.parent() == Some(create) && object.role() == UiObjectRole::NormalTexture
        })
        .map(|(index, _)| index)
        .ok_or_else(|| invalid_data("create-character button has no NormalTexture".to_owned()))?;
    if !manager
        .presentation()
        .members_in_draw_order()
        .iter()
        .any(|quad| quad.object_index() == normal_texture)
    {
        return Err(
            invalid_data("create-character button has no visible stock skin".to_owned()).into(),
        );
    }
    let button_text = manager
        .objects()
        .iter()
        .enumerate()
        .find(|(_, object)| {
            object.parent() == Some(create) && object.role() == UiObjectRole::ButtonText
        })
        .map(|(index, _)| index)
        .ok_or_else(|| invalid_data("create-character button has no ButtonText".to_owned()))?;
    if !visible_glyph_owners(manager).contains(&button_text) {
        return Err(invalid_data(
            "create-character button label produced no visible glyphs".to_owned(),
        )
        .into());
    }
    let mesh_objects = manager.render_plan().mesh().object_indices();
    if !mesh_objects.contains(&normal_texture) || !mesh_objects.contains(&button_text) {
        return Err(invalid_data(format!(
            "create-character skin/text did not reach the final mesh: normal={}, text={}",
            mesh_objects.contains(&normal_texture),
            mesh_objects.contains(&button_text)
        ))
        .into());
    }
    let mut texture_cache = BlpTextureCache::new();
    let texture_bindings = manager.load_blocking_render_textures(&mut texture_cache)?;

    let position = object_center(manager, create)?;
    let down = manager.pointer_button(position, UiPointerButton::Left, true)?;
    let up = manager.pointer_button(position, UiPointerButton::Left, false)?;
    if down.object_index() != Some(create)
        || up.object_index() != Some(create)
        || !up.click_activated()
    {
        return Err(invalid_data(format!(
            "create-character button did not receive stock pointer activation: down={down:?}, up={up:?}"
        ))
        .into());
    }
    manager.update(0.25)?;
    manager.update(0.25)?;
    if manager.current_screen() != "charcreate" || !object_is_shown(manager, "CharacterCreate")? {
        return Err(invalid_data(format!(
            "empty-account create button did not reach character creation: screen={}",
            manager.current_screen()
        ))
        .into());
    }
    let Some(UiGlueNetworkAction::SelectCharacter { index: 1 }) = manager.take_network_action()
    else {
        return Err(invalid_data(
            "empty selection did not publish stock's normalized first-row event".to_owned(),
        )
        .into());
    };
    println!(
        "empty character selection: default_model={} create_bounds={:?} quads={} batches={} resident_textures={} pending_textures={}",
        model_path,
        create_presentation_bounds,
        manager.presentation().member_count(),
        manager.render_plan().mesh().batches().len(),
        texture_bindings.resident_count(),
        texture_bindings.pending_count(),
    );
    Ok(())
}

fn report_login_presentation(manager: &GlueManager, resident_textures: usize) {
    println!(
        "login presentation: quads={} models={} backdrops={} resident_textures={resident_textures}",
        manager.presentation().member_count(),
        manager.presentation().models().len(),
        manager.backdrops().state_count(),
    );
    for model in manager.presentation().models() {
        println!(
            "model object={:?} path={} camera={} sequence={} scale={} bounds={:?}",
            manager.objects()[model.object_index()].name(),
            model.path(),
            model.camera(),
            model.sequence(),
            model.model_scale(),
            model.bounds()
        );
    }
}

fn validate_login_presentation(manager: &GlueManager) -> Result<(), Box<dyn Error>> {
    let background_path = AssetPath::new("Interface\\Tooltips\\UI-Tooltip-Background.blp")?;
    let edge_path = AssetPath::new("Interface\\Glues\\Common\\Glue-Tooltip-Border.blp")?;
    for name in ["AccountLoginAccountEdit", "AccountLoginPasswordEdit"] {
        let object_index = object_index(manager, name)?;
        let backdrop = manager
            .backdrops()
            .state(object_index)
            .ok_or_else(|| invalid_data(format!("{name} has no native backdrop state")))?;
        if backdrop.background() != Some(&background_path)
            || backdrop.edge() != Some(&edge_path)
            || backdrop.insets() != [10.0, 5.0, 4.0, 9.0]
            || backdrop.tile_size() != 16.0
            || backdrop.edge_size() != 16.0
        {
            return Err(invalid_data(format!("{name} backdrop differs from stock XML")).into());
        }
        let quads = manager
            .presentation()
            .members_in_draw_order()
            .iter()
            .filter(|quad| quad.object_index() == object_index)
            .collect::<Vec<_>>();
        if quads.len() != 29 {
            return Err(invalid_data(format!(
                "{name} produced {} backdrop quads instead of 29",
                quads.len()
            ))
            .into());
        }
        let background = quads
            .iter()
            .find(|quad| quad.source() == &UiTextureSource::Asset(background_path.clone()))
            .ok_or_else(|| invalid_data(format!("{name} has no backdrop background quad")))?;
        if (background.bounds().width() - 185.0).abs() > 0.000_01
            || (background.bounds().height() - 24.0).abs() > 0.000_01
        {
            return Err(invalid_data(format!("{name} background insets are incorrect")).into());
        }
    }

    let login_button = object_index(manager, "AccountLoginLoginButton")?;
    let normal_texture = manager
        .objects()
        .iter()
        .enumerate()
        .find(|(_, object)| {
            object.parent() == Some(login_button) && object.role() == UiObjectRole::NormalTexture
        })
        .map(|(index, _)| index)
        .ok_or_else(|| invalid_data("login button has no NormalTexture slot".to_owned()))?;
    let normal_quad = manager
        .presentation()
        .members_in_draw_order()
        .iter()
        .find(|quad| quad.object_index() == normal_texture)
        .ok_or_else(|| invalid_data("login button NormalTexture is not presented".to_owned()))?;
    if (normal_quad.bounds().width() - 220.0).abs() > 0.000_01
        || (normal_quad.bounds().height() - 45.0).abs() > 0.000_01
    {
        return Err(invalid_data("login button texture does not fill its owner".to_owned()).into());
    }
    let button_text = manager
        .objects()
        .iter()
        .enumerate()
        .find(|(_, object)| {
            object.parent() == Some(login_button) && object.role() == UiObjectRole::ButtonText
        })
        .map(|(index, _)| index)
        .ok_or_else(|| invalid_data("login button has no ButtonText slot".to_owned()))?;
    if !manager
        .glyphs()
        .quads_with_scroll(manager.geometry(), manager.scroll_frames())
        .iter()
        .any(|glyph| glyph.object_index() == button_text)
    {
        return Err(invalid_data("login button label produced no glyphs".to_owned()).into());
    }
    let mesh_objects = manager.render_plan().mesh().object_indices();
    let normal_draw = mesh_objects
        .iter()
        .position(|object| *object == normal_texture)
        .ok_or_else(|| invalid_data("login button skin has no mesh draw".to_owned()))?;
    let text_draw = mesh_objects
        .iter()
        .position(|object| *object == button_text)
        .ok_or_else(|| invalid_data("login button label has no mesh draw".to_owned()))?;
    if text_draw <= normal_draw {
        return Err(
            invalid_data("login button label is drawn behind its state skin".to_owned()).into(),
        );
    }

    let changed_options = object_index(manager, "ChangedOptionsDialogBackground")?;
    if manager
        .presentation()
        .members_in_draw_order()
        .iter()
        .any(|quad| quad.object_index() == changed_options)
    {
        return Err(invalid_data(
            "empty changed-options dialog survived its stock OnShow handler".to_owned(),
        )
        .into());
    }
    Ok(())
}

fn validate_character_selection(manager: &mut GlueManager) -> Result<(), Box<dyn Error>> {
    const CHARACTER_GUID: u64 = 0xAABB_CCDD_EEFF_0011;
    manager.set_network_status(UiGlueNetworkStatus::new(
        Some("Validation Realm".to_owned()),
        true,
    ));
    manager.set_character_directory(UiCharacterDirectory::new(
        vec![UiCharacterInfo::new(
            CHARACTER_GUID,
            "SolarityTester".to_owned(),
            "Human".to_owned(),
            1,
            "Human".to_owned(),
            "Warrior".to_owned(),
            1,
            80,
            Some("Dalaran".to_owned()),
            2,
            0,
            [0; 5],
            [UiCharacterEquipment::default(); 23],
            UiCharacterPetPreview::default(),
            0,
            0,
        )],
        "Orc".to_owned(),
    ));
    manager.dispatch_event(
        "SET_GLUE_SCREEN",
        &UiEventPayload::new([UiEventArgument::String("charselect".to_owned())])?,
    )?;
    manager.dispatch_event(
        "CHARACTER_LIST_UPDATE",
        &UiEventPayload::new([UiEventArgument::Integer(1)])?,
    )?;
    let quarter_changed = manager.update(0.25)?;
    if !quarter_changed || manager.current_screen() != "login" {
        return Err(invalid_data(
            format!(
                "stock login fade did not retain its pending screen at 0.25 seconds: changed={quarter_changed} screen={}",
                manager.current_screen(),
            ),
        )
        .into());
    }
    let login_alpha = manager
        .geometry()
        .region(object_index(manager, "AccountLoginUI")?)
        .ok_or_else(|| invalid_data("AccountLoginUI lost geometry during fade".to_owned()))?
        .effective_alpha();
    if (login_alpha - 0.5).abs() > 0.000_01 {
        return Err(invalid_data(format!(
            "stock login fade produced alpha {login_alpha} at 0.25 seconds"
        ))
        .into());
    }
    if !manager.update(0.25)? {
        return Err(invalid_data(
            "stock login fade completion did not mutate presentation".to_owned(),
        )
        .into());
    }
    let screen = manager.current_screen();
    let selection_shown = object_is_shown(manager, "CharacterSelectUI")?;
    let login_shown = object_is_shown(manager, "AccountLoginUI")?;
    if screen != "charselect" || !selection_shown || login_shown {
        return Err(invalid_data(format!(
            "stock character-selection event did not replace the login screen: screen={screen} selection_shown={selection_shown} login_shown={login_shown}"
        ))
        .into());
    }
    let model = manager
        .presentation()
        .models()
        .iter()
        .find(|model| manager.objects()[model.object_index()].name() == Some("CharacterSelect"))
        .ok_or_else(|| invalid_data("character selection has no visible ModelFFX".to_owned()))?;
    if model.path().as_str() != "INTERFACE\\GLUES\\MODELS\\UI_HUMAN\\UI_HUMAN.M2"
        || model.camera() != 0
        || model.sequence() != 0
        || (model.glow() - 0.15).abs() > 0.000_01
    {
        return Err(invalid_data(format!(
            "stock Human character background state is incomplete: {model:?}"
        ))
        .into());
    }
    let fog = model
        .fog()
        .ok_or_else(|| invalid_data("Human character background has no fog".to_owned()))?;
    if fog.color() != [0.8, 0.65, 0.73] || fog.range() != [0.0, 222.0] {
        return Err(invalid_data(format!(
            "Human character background fog differs from stock: {fog:?}"
        ))
        .into());
    }
    for (label, lights) in [
        ("background", model.background_lights()),
        ("character", model.character_lights()),
        ("pet", model.pet_lights()),
    ] {
        if lights.live().iter().flatten().count() != 3 || lights.ghost().iter().any(Option::is_some)
        {
            return Err(invalid_data(format!(
                "Human {label} light set does not retain the three stock live lights"
            ))
            .into());
        }
    }
    let Some(UiGlueNetworkAction::SelectCharacter { index }) = manager.take_network_action() else {
        return Err(invalid_data(
            "stock character-selection refresh did not select the first row".to_owned(),
        )
        .into());
    };
    if index != 1 {
        return Err(invalid_data(format!(
            "stock character-selection refresh selected row {index}"
        ))
        .into());
    }
    for expected in [
        UiGlueNetworkAction::ReadyForAccountDataTimes,
        UiGlueNetworkAction::RequestCharacterListUpdate,
        UiGlueNetworkAction::RequestRealmSplitInfo,
    ] {
        let Some(actual) = manager.take_network_action() else {
            return Err(invalid_data(format!(
                "stock character-selection OnShow omitted {expected:?}"
            ))
            .into());
        };
        if std::mem::discriminant(&actual) != std::mem::discriminant(&expected) {
            return Err(invalid_data(format!(
                "stock character-selection OnShow emitted {actual:?} before {expected:?}"
            ))
            .into());
        }
    }
    if manager.take_network_action().is_some() {
        return Err(invalid_data(
            "character-selection validation left an unexpected network action".to_owned(),
        )
        .into());
    }
    Ok(())
}

fn validate_login_input(manager: &mut GlueManager) -> Result<(), Box<dyn Error>> {
    let account_index = object_index(manager, "AccountLoginAccountEdit")?;
    let password_index = object_index(manager, "AccountLoginPasswordEdit")?;
    let label_index = object_index(manager, "AccountLoginAccountEditLabel")?;
    if !visible_glyph_owners(manager).contains(&label_index) {
        return Err(
            invalid_data("stock account label produced no visible glyphs".to_owned()).into(),
        );
    }
    if manager.focused_edit_box() != Some(account_index) {
        return Err(
            invalid_data("stock login did not focus the account EditBox".to_owned()).into(),
        );
    }
    let atlas_identity = manager.glyphs().identity();
    manager.text_input("VALIDATION_ACCOUNT")?;
    if manager.glyphs().identity() != atlas_identity {
        return Err(invalid_data(
            "baseline account input unnecessarily rebuilt the glyph atlas".to_owned(),
        )
        .into());
    }
    let account_glyphs = visible_glyph_owners(manager)
        .into_iter()
        .filter(|owner| *owner == account_index)
        .count();
    if account_glyphs != "VALIDATION_ACCOUNT".chars().count() {
        return Err(
            invalid_data(format!("account EditBox produced {account_glyphs} glyphs")).into(),
        );
    }
    if manager.keyboard_key("TAB", true, UiKeyboardModifiers::default())? != Some(account_index)
        || manager.focused_edit_box() != Some(password_index)
    {
        return Err(
            invalid_data("stock login Tab did not focus the password EditBox".to_owned()).into(),
        );
    }
    const VALIDATION_PASSWORD: &str = "validation-pass";
    manager.text_input(VALIDATION_PASSWORD)?;
    let password_glyphs = visible_glyph_owners(manager)
        .into_iter()
        .filter(|owner| *owner == password_index)
        .count();
    if password_glyphs != VALIDATION_PASSWORD.chars().count() {
        return Err(invalid_data(format!(
            "password EditBox produced {password_glyphs} masked glyphs"
        ))
        .into());
    }
    manager.keyboard_key("ENTER", true, UiKeyboardModifiers::default())?;
    let Some(UiGlueNetworkAction::Login(request)) = manager.take_network_action() else {
        return Err(
            invalid_data("stock login Enter did not emit a login request".to_owned()).into(),
        );
    };
    if request.account_name() != "VALIDATION_ACCOUNT"
        || request.password_bytes() != VALIDATION_PASSWORD.as_bytes()
    {
        return Err(invalid_data(
            "stock login request did not retain entered credentials".to_owned(),
        )
        .into());
    }
    let globals = manager.bundle().lua().globals();
    let password = globals.get::<mlua::Table>("AccountLoginPasswordEdit")?;
    let get_text = password.get::<mlua::Function>("GetText")?;
    if !get_text.call::<String>(password)?.is_empty() {
        return Err(
            invalid_data("stock login did not clear submitted password text".to_owned()).into(),
        );
    }
    Ok(())
}

fn visible_glyph_owners(manager: &GlueManager) -> Vec<usize> {
    manager
        .glyphs()
        .quads_with_scroll(manager.geometry(), manager.scroll_frames())
        .into_iter()
        .map(|quad| quad.object_index())
        .collect()
}

fn contains_change(changes: &[(String, String)], name: &str, value: &str) -> bool {
    changes
        .iter()
        .any(|(changed_name, changed_value)| changed_name == name && changed_value == value)
}

fn object_is_shown(manager: &GlueManager, name: &str) -> Result<bool, IoError> {
    let index = object_index(manager, name)?;
    Ok(manager
        .geometry()
        .region(index)
        .is_some_and(solarity_ui::UiRegionGeometry::effectively_shown))
}

fn accept_notice(
    manager: &mut GlueManager,
    scroll_name: &str,
    accept_name: &str,
    cvar: &str,
) -> Result<(), Box<dyn Error>> {
    let scroll_index = object_index(manager, scroll_name)?;
    let position = object_center(manager, scroll_index)?;
    let initial = manager
        .scroll_frames()
        .state(scroll_index)
        .ok_or_else(|| invalid_data(format!("{scroll_name} has no live scroll state")))?;
    println!(
        "{scroll_name}: offset={}, range={}",
        initial.offset().1,
        initial.range().1
    );
    if initial.offset().1 < initial.range().1 {
        if manager.pointer_wheel(position, -1.0)? != Some(scroll_index) {
            return Err(
                invalid_data(format!("{scroll_name} did not retain wheel targeting")).into(),
            );
        }
        let advanced = manager
            .scroll_frames()
            .state(scroll_index)
            .ok_or_else(|| invalid_data(format!("{scroll_name} disappeared after scrolling")))?;
        let step = advanced.offset().1 - initial.offset().1;
        if step <= 0.0 {
            return Err(invalid_data(format!(
                "{scroll_name} authored wheel callback did not advance"
            ))
            .into());
        }
        let slider_name = format!("{scroll_name}ScrollBar");
        let slider_index = object_index(manager, &slider_name)?;
        let track = manager
            .geometry()
            .region(slider_index)
            .ok_or_else(|| invalid_data(format!("{slider_name} has no geometry")))?
            .presentation_bounds();
        let slider_center = (
            track.left() + track.width() * 0.5,
            track.bottom() + track.height() * 0.5,
        );
        let slider_bottom = (slider_center.0, track.bottom());
        let down = manager.pointer_button(slider_center, UiPointerButton::Left, true)?;
        let motion = manager.pointer_motion(slider_bottom)?;
        let up = manager.pointer_button(slider_bottom, UiPointerButton::Left, false)?;
        if down.object_index() != Some(slider_index)
            || motion != Some(slider_index)
            || up.object_index() != Some(slider_index)
        {
            return Err(invalid_data(format!(
                "{slider_name} did not retain native drag capture: down={down:?}, motion={motion:?}, up={up:?}"
            ))
            .into());
        }
    }
    let state = manager
        .scroll_frames()
        .state(scroll_index)
        .ok_or_else(|| invalid_data(format!("{scroll_name} disappeared after scrolling")))?;
    if state.offset().1 < state.range().1 {
        return Err(invalid_data(format!("{scroll_name} did not reach its authored range")).into());
    }

    let accept_index = object_index(manager, accept_name)?;
    let accept_position = object_center(manager, accept_index)?;
    let enabled = {
        let globals = manager.bundle().lua().globals();
        let button = globals.get::<mlua::Table>(accept_name)?;
        let is_enabled = button.get::<mlua::Function>("IsEnabled")?;
        is_enabled.call::<bool>(button)?
    };
    println!(
        "{accept_name}: enabled={enabled}, shown={}",
        manager
            .geometry()
            .region(accept_index)
            .is_some_and(solarity_ui::UiRegionGeometry::effectively_shown)
    );
    let down = manager.pointer_button(accept_position, UiPointerButton::Left, true)?;
    let up = manager.pointer_button(accept_position, UiPointerButton::Left, false)?;
    if down.object_index() != Some(accept_index)
        || up.object_index() != Some(accept_index)
        || !up.click_activated()
    {
        return Err(invalid_data(format!(
            "{accept_name} did not receive captured activation: down={down:?}, up={up:?}"
        ))
        .into());
    }
    if manager.cvar_value(cvar).as_deref() != Some("1") {
        return Err(invalid_data(format!("{accept_name} did not persist {cvar}")).into());
    }
    Ok(())
}

fn object_index(manager: &GlueManager, name: &str) -> Result<usize, IoError> {
    manager
        .objects()
        .iter()
        .position(|object| object.name() == Some(name))
        .ok_or_else(|| invalid_data(format!("Glue object {name} is unavailable")))
}

fn object_center(manager: &GlueManager, object_index: usize) -> Result<(f64, f64), IoError> {
    let bounds = manager
        .geometry()
        .region(object_index)
        .ok_or_else(|| invalid_data(format!("Glue object {object_index} has no geometry")))?
        .presentation_bounds();
    Ok((
        bounds.left() + bounds.width() * 0.5,
        bounds.bottom() + bounds.height() * 0.5,
    ))
}

fn invalid_data(message: String) -> IoError {
    IoError::new(ErrorKind::InvalidData, message)
}

fn usage_error() -> IoError {
    IoError::new(
        ErrorKind::InvalidInput,
        "usage: validate_glue_interaction <Data> <locale>",
    )
}
