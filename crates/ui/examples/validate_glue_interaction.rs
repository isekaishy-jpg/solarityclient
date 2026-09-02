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
    UiGlueNetworkAction, UiGlueNetworkStatus, UiKeyboardModifiers, UiObjectKind, UiObjectRole,
    UiPointerButton, UiTextureSource,
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
    validate_login_presentation(&mut later)?;
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
    validate_visible_font_string_extents(manager, "character creation")?;
    validate_character_creation_choice_layout(manager)?;
    validate_character_creation_text_layout(manager)?;
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
    let create_model = manager
        .presentation()
        .models()
        .iter()
        .find(|model| manager.objects()[model.object_index()].name() == Some("CharacterCreate"))
        .ok_or_else(|| invalid_data("character creation has no visible ModelFFX".to_owned()))?;
    let create_bounds = create_model.bounds();
    let create_input_index = object_index(manager, "CharacterCreateFrame")?;
    let drag_start = (
        (create_bounds.left() + create_bounds.right()) * 0.5,
        (create_bounds.bottom() + create_bounds.top()) * 0.5,
    );
    let drag_finish = (drag_start.0 + 64.0, drag_start.1);
    let get_facing = globals.get::<mlua::Function>("GetCharacterCreateFacing")?;
    let initial_facing = get_facing.call::<f64>(())?;
    let down = manager.pointer_button(drag_start, UiPointerButton::Left, true)?;
    manager.pointer_motion(drag_finish)?;
    manager.update(0.016)?;
    let up = manager.pointer_button(drag_finish, UiPointerButton::Left, false)?;
    let dragged_facing = get_facing.call::<f64>(())?;
    let expected_facing = initial_facing + 64.0 * (720.0 / 768.0) * 0.6;
    if down.object_index() != Some(create_input_index)
        || up.object_index() != Some(create_input_index)
        || (dragged_facing - expected_facing).abs() > 0.001
    {
        return Err(invalid_data(format!(
            "character-creation model drag failed: down={down:?} up={up:?} facing={dragged_facing} expected={expected_facing}"
        ))
        .into());
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

fn validate_character_creation_choice_layout(manager: &GlueManager) -> Result<(), Box<dyn Error>> {
    // These are direct `x`/`y` attributes on shipped `<Anchor>` elements. They
    // cover a parent offset, both race columns, and both rows of class choices.
    for (name, expected) in [
        (
            "CharacterCreateConfigurationFrame",
            [9.0, 5.0, 265.0, 763.0],
        ),
        ("CharacterCreateRaceButton1", [68.0, 664.0, 106.0, 702.0]),
        ("CharacterCreateRaceButton6", [168.0, 664.0, 206.0, 702.0]),
        ("CharacterCreateClassButton1", [28.0, 262.0, 66.0, 300.0]),
        ("CharacterCreateClassButton10", [204.0, 218.0, 242.0, 256.0]),
    ] {
        let bounds = manager
            .geometry()
            .region(object_index(manager, name)?)
            .ok_or_else(|| invalid_data(format!("{name} has no geometry")))?
            .presentation_bounds();
        let actual = [bounds.left(), bounds.bottom(), bounds.right(), bounds.top()];
        if actual
            .iter()
            .zip(expected)
            .any(|(actual, expected)| (*actual - expected).abs() > 0.000_01)
        {
            return Err(invalid_data(format!(
                "{name} ignored its compact stock anchor offsets: {bounds:?}"
            ))
            .into());
        }
    }
    Ok(())
}

fn validate_character_creation_text_layout(manager: &GlueManager) -> Result<(), Box<dyn Error>> {
    let quads = manager
        .glyphs()
        .quads_with_scroll(manager.geometry(), manager.scroll_frames());
    for (name, scroll_name) in [
        ("CharacterCreateRaceText", "CharacterCreateRaceScrollFrame"),
        (
            "CharacterCreateClassText",
            "CharacterCreateClassScrollFrame",
        ),
    ] {
        let object = object_index(manager, name)?;
        let bounds = manager
            .geometry()
            .region(object)
            .ok_or_else(|| invalid_data(format!("{name} has no geometry")))?
            .presentation_bounds();
        let font_string = manager.bundle().lua().globals().get::<mlua::Table>(name)?;
        let measured_height = font_string
            .get::<mlua::Function>("GetStringHeight")?
            .call::<f64>(font_string.clone())?;
        let (measured_width, field_height) = font_string
            .get::<mlua::Function>("GetFieldSize")?
            .call::<(f64, f64)>(font_string)?;
        let glyph_bounds = quads
            .iter()
            .filter(|glyph| glyph.object_index() == object)
            .map(|glyph| glyph.bounds())
            .collect::<Vec<_>>();
        let glyph_top = glyph_bounds
            .iter()
            .map(|glyph| glyph[3])
            .reduce(f32::max)
            .ok_or_else(|| invalid_data(format!("{name} has no glyphs")))?;
        let glyph_bottom = glyph_bounds
            .iter()
            .map(|glyph| glyph[1])
            .reduce(f32::min)
            .ok_or_else(|| invalid_data(format!("{name} has no glyphs")))?;
        if (bounds.width() - 220.0).abs() > 0.01
            || bounds.height() < 40.0
            || measured_width > 222.0
            || (measured_height - field_height).abs() > 0.01
            || glyph_top - glyph_bottom < 39.0
        {
            return Err(invalid_data(format!(
                "{name} did not retain stock fixed-width wrapping: bounds={bounds:?} measured=({measured_width}, {measured_height}) glyph_span={}",
                glyph_top - glyph_bottom,
            ))
            .into());
        }
        let overflow = glyph_bounds.iter().any(|glyph| {
            f64::from(glyph[0]) < bounds.left() - 2.0 || f64::from(glyph[2]) > bounds.right() + 2.0
        });
        if overflow {
            return Err(invalid_data(format!(
                "{name} emitted wrapped glyphs outside its fixed-width field"
            ))
            .into());
        }
        let viewport = manager
            .geometry()
            .region(object_index(manager, scroll_name)?)
            .ok_or_else(|| invalid_data(format!("{scroll_name} has no geometry")))?
            .presentation_bounds();
        let viewport_overflow = glyph_bounds.iter().find(|glyph| {
            f64::from(glyph[0]) < viewport.left() - 0.01
                || f64::from(glyph[1]) < viewport.bottom() - 0.01
                || f64::from(glyph[2]) > viewport.right() + 0.01
                || f64::from(glyph[3]) > viewport.top() + 0.01
        });
        if let Some(glyph) = viewport_overflow {
            return Err(invalid_data(format!(
                "{name} emitted glyph {glyph:?} outside {scroll_name}'s stock viewport {viewport:?}"
            ))
            .into());
        }
    }
    Ok(())
}

fn validate_empty_character_selection(manager: &mut GlueManager) -> Result<(), Box<dyn Error>> {
    manager.set_character_directory(UiCharacterDirectory::new(Vec::new(), "Orc".to_owned()));
    manager.dispatch_event(
        "CHARACTER_LIST_UPDATE",
        &UiEventPayload::new([UiEventArgument::Integer(0)])?,
    )?;
    validate_visible_font_string_extents(manager, "empty character selection")?;

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
    let label_bounds = manager
        .glyphs()
        .quads_with_scroll(manager.geometry(), manager.scroll_frames())
        .into_iter()
        .filter(|glyph| glyph.object_index() == button_text)
        .map(|glyph| glyph.bounds())
        .fold(None, |extent: Option<[f32; 2]>, bounds| {
            Some(extent.map_or([bounds[1], bounds[3]], |[bottom, top]| {
                [bottom.min(bounds[1]), top.max(bounds[3])]
            }))
        })
        .ok_or_else(|| invalid_data("create-character button label has no bounds".to_owned()))?;
    if label_bounds[1] - label_bounds[0] > create_presentation_bounds.height() as f32 * 0.55 {
        return Err(invalid_data(format!(
            "create-character ButtonText wrapped instead of retaining stock's single line: {label_bounds:?}"
        ))
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

fn validate_login_presentation(manager: &mut GlueManager) -> Result<(), Box<dyn Error>> {
    let background_path = AssetPath::new("Interface\\Tooltips\\UI-Tooltip-Background.blp")?;
    let edge_path = AssetPath::new("Interface\\Glues\\Common\\Glue-Tooltip-Border.blp")?;
    let dialog_html = manager
        .bundle()
        .lua()
        .globals()
        .get::<mlua::Table>("GlueDialogHTML")?;
    let (_, _, _, dialog_text_height) = dialog_html
        .get::<mlua::Function>("GetBoundsRect")?
        .call::<(f64, f64, f64, f64)>(dialog_html)?;
    if !dialog_text_height.is_finite() || dialog_text_height <= 0.0 {
        return Err(invalid_data(format!(
            "GlueDialogHTML returned invalid content bounds height {dialog_text_height}"
        ))
        .into());
    }
    validate_automatic_font_string_extents(manager)?;
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
    let button_text_colors = manager
        .glyphs()
        .quads_with_scroll(manager.geometry(), manager.scroll_frames())
        .into_iter()
        .filter(|glyph| glyph.object_index() == button_text)
        .map(|glyph| glyph.color())
        .collect::<Vec<_>>();
    if !button_text_colors.contains(&[0.0, 0.0, 0.0, 1.0]) {
        return Err(
            invalid_data("login button label omitted its authored outline".to_owned()).into(),
        );
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
    let normal_colors = manager
        .glyphs()
        .quads_with_scroll(manager.geometry(), manager.scroll_frames())
        .into_iter()
        .filter(|glyph| glyph.object_index() == button_text)
        .map(|glyph| glyph.color())
        .filter(|color| *color != [0.0, 0.0, 0.0, 1.0])
        .collect::<Vec<_>>();
    manager.pointer_motion(object_center(manager, login_button)?)?;
    let highlight_colors = manager
        .glyphs()
        .quads_with_scroll(manager.geometry(), manager.scroll_frames())
        .into_iter()
        .filter(|glyph| glyph.object_index() == button_text)
        .map(|glyph| glyph.color())
        .filter(|color| *color != [0.0, 0.0, 0.0, 1.0])
        .collect::<Vec<_>>();
    if normal_colors.is_empty() || normal_colors == highlight_colors {
        return Err(invalid_data(
            "login button hover did not select its authored HighlightFont".to_owned(),
        )
        .into());
    }
    manager.pointer_motion((-1.0, -1.0))?;

    let (original_text, original_width, formatted_width, plain_width) = {
        let button = manager
            .bundle()
            .lua()
            .globals()
            .get::<mlua::Table>("AccountLoginLoginButton")?;
        let label = button
            .get::<mlua::Function>("GetFontString")?
            .call::<mlua::Table>(button)?;
        let original = label
            .get::<mlua::Function>("GetText")?
            .call::<Option<String>>(label.clone())?;
        let width = label
            .get::<mlua::Function>("GetStringWidth")?
            .call::<f64>(label.clone())?;
        let set_text = label.get::<mlua::Function>("SetText")?;
        set_text.call::<()>((label.clone(), "|cffff0000R|rW"))?;
        let formatted_width = label
            .get::<mlua::Function>("GetStringWidth")?
            .call::<f64>(label.clone())?;
        set_text.call::<()>((label.clone(), "RW"))?;
        let plain_width = label
            .get::<mlua::Function>("GetStringWidth")?
            .call::<f64>(label.clone())?;
        set_text.call::<()>((label, "|cffff0000R|rW"))?;
        (original, width, formatted_width, plain_width)
    };
    if !original_width.is_finite() || original_width <= 0.0 {
        return Err(invalid_data(format!(
            "login button FontString returned invalid stock width {original_width}"
        ))
        .into());
    }
    if (formatted_width - plain_width).abs() > 0.000_01 {
        return Err(invalid_data(format!(
            "FontString width included inline color controls: formatted={formatted_width} plain={plain_width}"
        ))
        .into());
    }
    manager.pointer_motion(object_center(manager, login_button)?)?;
    let formatted_colors = manager
        .glyphs()
        .quads_with_scroll(manager.geometry(), manager.scroll_frames())
        .into_iter()
        .filter(|glyph| glyph.object_index() == button_text)
        .map(|glyph| glyph.color())
        .filter(|color| *color != [0.0, 0.0, 0.0, 1.0])
        .collect::<Vec<_>>();
    if formatted_colors.len() != 2
        || formatted_colors[0] != [1.0, 0.0, 0.0, 1.0]
        || formatted_colors[1] != highlight_colors[0]
    {
        return Err(invalid_data(format!(
            "inline Glue color markup reached glyphs incorrectly: {formatted_colors:?}"
        ))
        .into());
    }
    {
        let button = manager
            .bundle()
            .lua()
            .globals()
            .get::<mlua::Table>("AccountLoginLoginButton")?;
        let label = button
            .get::<mlua::Function>("GetFontString")?
            .call::<mlua::Table>(button)?;
        label
            .get::<mlua::Function>("SetText")?
            .call::<()>((label, original_text))?;
    }
    manager.pointer_motion((-1.0, -1.0))?;

    {
        let button = manager
            .bundle()
            .lua()
            .globals()
            .get::<mlua::Table>("AccountLoginLoginButton")?;
        button
            .get::<mlua::Function>("SetDisabledTextColor")?
            .call::<()>((button.clone(), 0.2, 0.3, 0.4, 0.5))?;
        button
            .get::<mlua::Function>("Disable")?
            .call::<()>(button)?;
    }
    manager.pointer_motion(object_center(manager, login_button)?)?;
    let disabled_colors = manager
        .glyphs()
        .quads_with_scroll(manager.geometry(), manager.scroll_frames())
        .into_iter()
        .filter(|glyph| glyph.object_index() == button_text)
        .map(|glyph| glyph.color())
        .collect::<Vec<_>>();
    if disabled_colors.is_empty()
        || disabled_colors
            .iter()
            .any(|color| *color != [0.2, 0.3, 0.4, 0.5] && *color != [0.0, 0.0, 0.0, 1.0])
        || !disabled_colors.contains(&[0.2, 0.3, 0.4, 0.5])
    {
        return Err(invalid_data(format!(
            "disabled Glue button ignored its authored text color: {disabled_colors:?}"
        ))
        .into());
    }
    {
        let button = manager
            .bundle()
            .lua()
            .globals()
            .get::<mlua::Table>("AccountLoginLoginButton")?;
        button.get::<mlua::Function>("Enable")?.call::<()>(button)?;
    }
    manager.pointer_motion((-1.0, -1.0))?;

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

fn validate_automatic_font_string_extents(manager: &GlueManager) -> Result<(), Box<dyn Error>> {
    let version = object_index(manager, "AccountLoginVersion")?;
    let version_bounds = manager
        .geometry()
        .region(version)
        .ok_or_else(|| invalid_data("AccountLoginVersion has no geometry".to_owned()))?
        .presentation_bounds();
    if version_bounds.width() <= 0.0 || version_bounds.height() <= 0.0 {
        return Err(invalid_data(format!(
            "automatic version FontString retained empty bounds: {version_bounds:?}"
        ))
        .into());
    }
    let version_bottom = manager
        .glyphs()
        .quads_with_scroll(manager.geometry(), manager.scroll_frames())
        .iter()
        .filter(|glyph| glyph.object_index() == version)
        .map(|glyph| glyph.bounds()[1])
        .reduce(f32::min)
        .ok_or_else(|| invalid_data("AccountLoginVersion has no visible glyphs".to_owned()))?;
    if version_bottom < 0.0 {
        return Err(invalid_data(format!(
            "bottom-anchored version text is clipped below the canvas: {version_bottom}"
        ))
        .into());
    }

    let launcher = object_index(manager, "AccountLoginShowLauncher")?;
    let launcher_label = manager
        .children(launcher)
        .and_then(|children| {
            children
                .iter()
                .copied()
                .find(|index| manager.objects()[*index].kind() == UiObjectKind::FontString)
        })
        .ok_or_else(|| invalid_data("Show Launcher has no FontString child".to_owned()))?;
    let label_bounds = manager
        .geometry()
        .region(launcher_label)
        .ok_or_else(|| invalid_data("Show Launcher label has no geometry".to_owned()))?
        .presentation_bounds();
    if label_bounds.width() <= 0.0 || label_bounds.height() <= 0.0 || label_bounds.left() < 0.0 {
        return Err(invalid_data(format!(
            "automatic Show Launcher label bounds are clipped: {label_bounds:?}"
        ))
        .into());
    }
    validate_visible_font_string_extents(manager, "login")?;
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
    validate_visible_font_string_extents(manager, "character selection")?;
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
    let select_input_index = object_index(manager, "CharacterSelectUI")?;
    let select_bounds = model.bounds();
    let drag_start = (
        (select_bounds.left() + select_bounds.right()) * 0.5,
        (select_bounds.bottom() + select_bounds.top()) * 0.5,
    );
    let drag_finish = (drag_start.0 + 64.0, drag_start.1);
    let get_facing = manager
        .bundle()
        .lua()
        .globals()
        .get::<mlua::Function>("GetCharacterSelectFacing")?;
    let initial_facing = get_facing.call::<f64>(())?;
    let down = manager.pointer_button(drag_start, UiPointerButton::Left, true)?;
    manager.pointer_motion(drag_finish)?;
    manager.update(0.016)?;
    let up = manager.pointer_button(drag_finish, UiPointerButton::Left, false)?;
    let dragged_facing = get_facing.call::<f64>(())?;
    let expected_facing = initial_facing + 64.0 * (720.0 / 768.0) * 0.6;
    if down.object_index() != Some(select_input_index)
        || up.object_index() != Some(select_input_index)
        || (dragged_facing - expected_facing).abs() > 0.001
    {
        return Err(invalid_data(format!(
            "character-selection model drag failed: down={down:?} up={up:?} facing={dragged_facing} expected={expected_facing}"
        ))
        .into());
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
    let row = object_index(manager, "CharSelectCharacterButton1")?;
    let row_position = object_center(manager, row)?;
    let double_down =
        manager.pointer_button_with_click_count(row_position, UiPointerButton::Left, true, 2)?;
    let double_up =
        manager.pointer_button_with_click_count(row_position, UiPointerButton::Left, false, 2)?;
    if double_down.object_index() != Some(row)
        || double_up.object_index() != Some(row)
        || !double_up.click_activated()
    {
        return Err(invalid_data(format!(
            "character row did not accept a native double click: down={double_down:?} up={double_up:?}"
        ))
        .into());
    }
    let Some(UiGlueNetworkAction::EnterWorld {
        guid: CHARACTER_GUID,
    }) = manager.take_network_action()
    else {
        return Err(invalid_data(
            "character-row double click did not run OnDoubleClick world entry".to_owned(),
        )
        .into());
    };
    if manager.take_network_action().is_some() {
        return Err(invalid_data(
            "character-row double click emitted an unexpected second action".to_owned(),
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

fn validate_visible_font_string_extents(
    manager: &GlueManager,
    screen: &str,
) -> Result<(), Box<dyn Error>> {
    let glyph_owners = visible_glyph_owners(manager);
    for (object_index, object) in manager.objects().iter().enumerate() {
        if object.kind() != UiObjectKind::FontString || !glyph_owners.contains(&object_index) {
            continue;
        }
        let bounds = manager
            .geometry()
            .region(object_index)
            .ok_or_else(|| {
                invalid_data(format!(
                    "{screen} FontString {object_index} has no geometry"
                ))
            })?
            .presentation_bounds();
        if bounds.width() <= 0.0 || bounds.height() <= 0.0 {
            return Err(invalid_data(format!(
                "{screen} visible FontString {:?} retained empty bounds: {bounds:?}",
                object.name()
            ))
            .into());
        }
    }
    Ok(())
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
