//! External stock-compatibility tests for ECS-to-model appearance resolution.

use std::error::Error;

use glam::Vec3;
use solarity_asset::{
    AppearanceError, ArchiveCatalog, AssetStore, CharacterAppearanceCatalog, ClientDataRoot,
    CreatureCatalog, Locale,
};
use solarity_ecs::{ActiveWorld, ObjectKind, WorldBootstrap, WorldMapId};
use solarity_systems::{UnitModelAppearanceError, project_object_fields, resolve_unit_model};

use crate::support::{Fixture, FixtureFile};

/// Player ECS fields resolve the active body, customization, and mount independently.
#[test]
fn player_model_resolution_joins_stock_ecs_and_dbc_keys() -> Result<(), Box<dyn Error>> {
    let tables = appearance_tables();
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "DBFilesClient\\CreatureDisplayInfo.dbc",
            bytes: &tables.creature_displays,
        },
        FixtureFile {
            path: "DBFilesClient\\CreatureDisplayInfoExtra.dbc",
            bytes: &tables.creature_extras,
        },
        FixtureFile {
            path: "DBFilesClient\\CreatureModelData.dbc",
            bytes: &tables.creature_models,
        },
        FixtureFile {
            path: "DBFilesClient\\CharSections.dbc",
            bytes: &tables.character_sections,
        },
        FixtureFile {
            path: "DBFilesClient\\CharHairGeosets.dbc",
            bytes: &tables.hair_geosets,
        },
        FixtureFile {
            path: "DBFilesClient\\CharacterFacialHairStyles.dbc",
            bytes: &tables.facial_hair,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let creatures = CreatureCatalog::load(&mut store)?;
    let characters = CharacterAppearanceCatalog::load(&mut store)?;

    let guid = 0x42;
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        guid,
        "Solarion",
        Vec3::ZERO,
        0.0,
    ));
    let fields = [
        (4, 1.25_f32.to_bits()),
        (23, u32::from_le_bytes([1, 6, 0, 0])),
        (67, 20_000),
        (68, 20_001),
        (69, 14_307),
        (153, u32::from_le_bytes([2, 3, 4, 5])),
        (154, u32::from_le_bytes([6, 0, 0, 0])),
    ];
    world.create_object(guid, ObjectKind::Player, None, fields)?;
    project_object_fields(&mut world, guid, fields)?;

    let resolved = resolve_unit_model(&world, guid, &creatures, &characters)?;

    assert_eq!(resolved.guid(), guid);
    assert_eq!(resolved.object_scale(), 1.25);
    assert_eq!(resolved.native_display_id(), 20_001);
    assert_eq!(resolved.player_class_id(), Some(6));
    assert_eq!(
        resolved.body().model_path().as_str(),
        "CHARACTER\\HUMAN\\MALE\\HUMANMALE.M2"
    );
    let character = resolved.character().ok_or("player composition is absent")?;
    assert_eq!(character.skin().id(), 10);
    assert_eq!(character.face().map(|section| section.id()), Some(11));
    assert_eq!(character.hair().map(|section| section.id()), Some(13));
    assert_eq!(character.geosets().hair(), 12);
    assert_eq!(
        resolved
            .mount()
            .ok_or("mount model is absent")?
            .model_path()
            .as_str(),
        "CREATURE\\HORSE\\HORSE.M2"
    );

    // A remote non-DK may carry a DK-only skin byte. The render bank accepts
    // that authored row; class eligibility applies only to creation choices.
    let remote = 0x43;
    let remote_fields = [
        (23, u32::from_le_bytes([2, 1, 0, 0])),
        (67, 20_000),
        (68, 20_000),
        (153, u32::from_le_bytes([17, 0, 0, 0])),
        (154, 0),
    ];
    world.create_object(remote, ObjectKind::Player, None, remote_fields)?;
    project_object_fields(&mut world, remote, remote_fields)?;
    let remote_model = resolve_unit_model(&world, remote, &creatures, &characters)?;
    assert_eq!(remote_model.player_class_id(), Some(1));
    let remote_character = remote_model
        .character()
        .ok_or("remote composition is absent")?;
    assert_eq!(remote_character.skin().id(), 10_627);
    assert_eq!(remote_character.skin().flags(), 5);
    assert_eq!(remote_character.race_id(), 2);
    assert_eq!(characters.player_skin_colors_for_class(2, 0, 1), []);

    // A later morph value must not reuse the native display or another row.
    let invalid_display = [(67, 99_999)];
    world.update_fields(guid, invalid_display)?;
    project_object_fields(&mut world, guid, invalid_display)?;
    assert_eq!(
        resolve_unit_model(&world, guid, &creatures, &characters).err(),
        Some(UnitModelAppearanceError::Asset(
            AppearanceError::MissingCreatureDisplay { display_id: 99_999 }
        ))
    );
    Ok(())
}

/// A non-unit category is rejected before any unit component or DBC fallback.
#[test]
fn model_resolution_rejects_non_unit_objects() -> Result<(), Box<dyn Error>> {
    let tables = appearance_tables();
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "DBFilesClient\\CreatureDisplayInfo.dbc",
            bytes: &tables.creature_displays,
        },
        FixtureFile {
            path: "DBFilesClient\\CreatureDisplayInfoExtra.dbc",
            bytes: &tables.creature_extras,
        },
        FixtureFile {
            path: "DBFilesClient\\CreatureModelData.dbc",
            bytes: &tables.creature_models,
        },
        FixtureFile {
            path: "DBFilesClient\\CharSections.dbc",
            bytes: &tables.character_sections,
        },
        FixtureFile {
            path: "DBFilesClient\\CharHairGeosets.dbc",
            bytes: &tables.hair_geosets,
        },
        FixtureFile {
            path: "DBFilesClient\\CharacterFacialHairStyles.dbc",
            bytes: &tables.facial_hair,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let creatures = CreatureCatalog::load(&mut store)?;
    let characters = CharacterAppearanceCatalog::load(&mut store)?;
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        1,
        "Solarion",
        Vec3::ZERO,
        0.0,
    ));
    world.create_object(2, ObjectKind::GameObject, None, [])?;

    assert_eq!(
        resolve_unit_model(&world, 2, &creatures, &characters).err(),
        Some(UnitModelAppearanceError::NotUnit { guid: 2 })
    );
    assert_eq!(
        resolve_unit_model(&world, 3, &creatures, &characters).err(),
        Some(UnitModelAppearanceError::UnknownObject { guid: 3 })
    );
    Ok(())
}

/// Complete fixture tables used by model-resolution tests.
struct AppearanceTables {
    creature_displays: Vec<u8>,
    creature_extras: Vec<u8>,
    creature_models: Vec<u8>,
    character_sections: Vec<u8>,
    hair_geosets: Vec<u8>,
    facial_hair: Vec<u8>,
}

/// Creates exact build-12340 rows for one human player and one mount.
fn appearance_tables() -> AppearanceTables {
    let mut display_strings = vec![0];
    let human_skin = append_string(&mut display_strings, "HumanMaleSkin.blp");
    let horse_skin = append_string(&mut display_strings, "HorseBrown.blp");
    let displays = [
        20_000,
        7,
        0,
        0,
        1.0_f32.to_bits(),
        255,
        human_skin,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        14_307,
        8,
        0,
        0,
        1.0_f32.to_bits(),
        255,
        horse_skin,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    ];
    let creature_displays = create_wdbc(2, 16, &displays, &display_strings);
    let creature_extras = create_wdbc(0, 21, &[], b"\0");

    let mut model_strings = vec![0];
    let human_model = append_string(&mut model_strings, "Character\\Human\\Male\\HumanMale.m2");
    let horse_model = append_string(&mut model_strings, "Creature\\Horse\\Horse.m2");
    let mut models = Vec::new();
    models.extend(model_fields(7, human_model));
    models.extend(model_fields(8, horse_model));
    let creature_models = create_wdbc(2, 28, &models, &model_strings);

    let mut section_strings = vec![0];
    let skin = append_string(&mut section_strings, "Character\\Human\\Male\\Skin.blp");
    let extra = append_string(
        &mut section_strings,
        "Character\\Human\\Male\\SkinExtra.blp",
    );
    let face_lower = append_string(
        &mut section_strings,
        "Character\\Human\\Male\\FaceLower.blp",
    );
    let face_upper = append_string(
        &mut section_strings,
        "Character\\Human\\Male\\FaceUpper.blp",
    );
    let facial_lower = append_string(
        &mut section_strings,
        "Character\\Human\\Male\\FacialLower.blp",
    );
    let facial_upper = append_string(
        &mut section_strings,
        "Character\\Human\\Male\\FacialUpper.blp",
    );
    let hair = append_string(&mut section_strings, "Character\\Human\\Male\\Hair.blp");
    let hair_lower = append_string(
        &mut section_strings,
        "Character\\Human\\Male\\HairLower.blp",
    );
    let hair_upper = append_string(
        &mut section_strings,
        "Character\\Human\\Male\\HairUpper.blp",
    );
    let underwear_lower = append_string(
        &mut section_strings,
        "Character\\Human\\Male\\UnderwearLower.blp",
    );
    let underwear_upper = append_string(
        &mut section_strings,
        "Character\\Human\\Male\\UnderwearUpper.blp",
    );
    let sections = [
        10,
        1,
        0,
        0,
        skin,
        extra,
        0,
        17,
        0,
        2,
        11,
        1,
        0,
        1,
        face_lower,
        face_upper,
        0,
        1,
        3,
        2,
        12,
        1,
        0,
        2,
        facial_lower,
        facial_upper,
        0,
        1,
        6,
        5,
        13,
        1,
        0,
        3,
        hair,
        hair_lower,
        hair_upper,
        // Non-player flags do not exclude a section from the rendering bank.
        18,
        4,
        5,
        14,
        1,
        0,
        4,
        underwear_lower,
        underwear_upper,
        0,
        1,
        0,
        2,
    ];
    let mut sections = sections.to_vec();
    for (id, base, texture) in [
        (10_627, 0, skin),
        (10_628, 1, face_lower),
        (10_631, 4, underwear_lower),
    ] {
        sections.extend_from_slice(&[id, 2, 0, base, texture, 0, 0, 5, 0, 17]);
    }
    let character_sections = create_wdbc(8, 10, &sections, &section_strings);
    let hair_geosets = create_wdbc(1, 6, &[90, 1, 0, 4, 12, 1], b"\0");
    let facial_hair = create_wdbc(1, 8, &[1, 0, 6, 1, 2, 3, 4, 5], b"\0");
    AppearanceTables {
        creature_displays,
        creature_extras,
        creature_models,
        character_sections,
        hair_geosets,
        facial_hair,
    }
}

/// Produces one complete 28-field creature-model row.
fn model_fields(id: u32, path_offset: u32) -> [u32; 28] {
    [
        id,
        0,
        path_offset,
        0,
        1.0_f32.to_bits(),
        0,
        0,
        0.0_f32.to_bits(),
        0.0_f32.to_bits(),
        0.0_f32.to_bits(),
        0,
        0,
        0,
        0,
        0.0_f32.to_bits(),
        0.0_f32.to_bits(),
        0.0_f32.to_bits(),
        0.0_f32.to_bits(),
        0.0_f32.to_bits(),
        0.0_f32.to_bits(),
        0.0_f32.to_bits(),
        0.0_f32.to_bits(),
        0.0_f32.to_bits(),
        1.0_f32.to_bits(),
        1.0_f32.to_bits(),
        0.0_f32.to_bits(),
        0.0_f32.to_bits(),
        0.0_f32.to_bits(),
    ]
}

/// Generates one fixed-layout WDBC table.
fn create_wdbc(
    record_count: u32,
    field_count: u32,
    fields: &[u32],
    string_block: &[u8],
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(20 + fields.len() * 4 + string_block.len());
    bytes.extend_from_slice(b"WDBC");
    bytes.extend_from_slice(&record_count.to_le_bytes());
    bytes.extend_from_slice(&field_count.to_le_bytes());
    bytes.extend_from_slice(&(field_count * 4).to_le_bytes());
    bytes.extend_from_slice(&(string_block.len() as u32).to_le_bytes());
    for field in fields {
        bytes.extend_from_slice(&field.to_le_bytes());
    }
    bytes.extend_from_slice(string_block);
    bytes
}

/// Appends a NUL-terminated fixture string and returns its table offset.
fn append_string(block: &mut Vec<u8>, value: &str) -> u32 {
    let offset = block.len() as u32;
    block.extend_from_slice(value.as_bytes());
    block.push(0);
    offset
}
