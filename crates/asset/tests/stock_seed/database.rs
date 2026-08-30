//! External stock-compatibility tests for WDBC bootstrap.

use std::error::Error;
use std::path::Path;

use solarity_asset::{
    AppearanceError, ArchiveCatalog, AreaTableCatalog, AssetError, AssetPath, AssetStore,
    CharacterAppearanceCatalog, CharacterClassCatalog, CharacterCustomization,
    CharacterRaceCatalog, ClientDataRoot, CreatureCatalog, HelmetGeosetVisibilityCatalog,
    InventoryType, ItemDefinitionCatalog, ItemDisplayCatalog, Locale, M2TextureKind, MapCatalog,
    MapKind, WdbcTable,
};

use crate::support::{Fixture, FixtureFile};

/// WDBC parsing begins only after ordinary stock archive resolution completes.
#[test]
fn wdbc_bootstrap_uses_arbitrary_file_precedence() -> Result<(), Box<dyn Error>> {
    let base_table = create_wdbc(1, 2, &[1, 0], b"base\0");
    let patch_table = create_wdbc(1, 2, &[7, 11], b"patched\0");
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\Test.dbc",
            bytes: &base_table,
        },
        FixtureFile {
            archive: "patch-2.MPQ",
            path: "DBFilesClient\\Test.dbc",
            bytes: &patch_table,
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let path = AssetPath::new("DBFilesClient/Test.dbc")?;

    let table = WdbcTable::load(&mut store, &path)?;

    assert_eq!(table.path(), &path);
    assert_eq!(table.source().relative_path(), Path::new("patch-2.MPQ"));
    assert_eq!(table.header().record_count(), 1);
    assert_eq!(table.header().field_count(), 2);
    assert_eq!(table.header().record_size(), 8);
    assert_eq!(table.header().string_block_size(), 8);
    assert_eq!(table.record(0), Some([7, 0, 0, 0, 11, 0, 0, 0].as_slice()));
    assert_eq!(table.record(1), None);
    assert_eq!(table.string_bytes(0), Some(b"patched".as_slice()));
    Ok(())
}

/// A truncated WDBC entry fails at the database boundary with its asset path.
#[test]
fn truncated_wdbc_is_rejected() -> Result<(), Box<dyn Error>> {
    let mut table = create_wdbc(1, 1, &[42], b"\0");
    table.truncate(table.len() - 1);
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\Broken.dbc",
        bytes: &table,
    }])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let path = AssetPath::new("DBFilesClient/Broken.dbc")?;

    assert!(matches!(
        WdbcTable::load(&mut store, &path),
        Err(AssetError::DatabaseDecode { path: failed, .. }) if failed == path
    ));
    Ok(())
}

/// Exact build-12340 creature tables resolve update-field display IDs to M2 metadata.
#[test]
fn creature_catalog_decodes_stock_display_and_model_layouts() -> Result<(), Box<dyn Error>> {
    let mut display_strings = vec![0];
    let skin_1 = append_string(&mut display_strings, "BearSkinBrown.blp");
    let skin_2 = append_string(&mut display_strings, "BearSkinBlack.blp");
    let portrait = append_string(&mut display_strings, "BearPortrait");
    let display_fields = [
        20_000,
        7,
        400,
        55,
        1.25_f32.to_bits(),
        255,
        skin_1,
        skin_2,
        0,
        portrait,
        2,
        3,
        4,
        5,
        0x0012_0304,
        6,
    ];
    let display_table = create_wdbc(1, 16, &display_fields, &display_strings);

    let mut extra_strings = vec![0];
    let baked_texture = append_string(&mut extra_strings, "Textures\\BakedNpc.blp");
    let extra_fields = [
        55,
        4,
        1,
        2,
        3,
        4,
        5,
        6,
        101,
        102,
        103,
        104,
        105,
        106,
        107,
        108,
        109,
        110,
        111,
        0x20,
        baked_texture,
    ];
    let extra_table = create_wdbc(1, 21, &extra_fields, &extra_strings);

    let mut model_strings = vec![0];
    let model_name = append_string(&mut model_strings, "Creature\\Bear\\Bear.m2");
    let model_fields = [
        7,
        0x10,
        model_name,
        2,
        0.75_f32.to_bits(),
        3,
        9,
        1.5_f32.to_bits(),
        2.5_f32.to_bits(),
        0.5_f32.to_bits(),
        12,
        13,
        14,
        15,
        1.25_f32.to_bits(),
        2.25_f32.to_bits(),
        3.25_f32.to_bits(),
        (-1.0_f32).to_bits(),
        (-2.0_f32).to_bits(),
        (-3.0_f32).to_bits(),
        1.0_f32.to_bits(),
        2.0_f32.to_bits(),
        3.0_f32.to_bits(),
        1.1_f32.to_bits(),
        1.2_f32.to_bits(),
        1.3_f32.to_bits(),
        1.4_f32.to_bits(),
        1.5_f32.to_bits(),
    ];
    let model_table = create_wdbc(1, 28, &model_fields, &model_strings);
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\CreatureDisplayInfo.dbc",
            bytes: &display_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\CreatureModelData.dbc",
            bytes: &model_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\CreatureDisplayInfoExtra.dbc",
            bytes: &extra_table,
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    let catalog = CreatureCatalog::load(&mut store)?;
    let display = catalog
        .display(20_000)
        .ok_or("display row was not indexed")?;
    assert_eq!(display.model_id(), 7);
    assert_eq!(display.sound_id(), 400);
    assert_eq!(display.extended_display_info_id(), 55);
    assert_eq!(display.model_scale(), 1.25);
    assert_eq!(display.model_alpha(), 255);
    assert_eq!(
        display.texture_variations(),
        ["BearSkinBrown.blp", "BearSkinBlack.blp", ""]
    );
    assert_eq!(display.portrait_texture_name(), "BearPortrait");
    assert_eq!(display.size_class(), 2);
    assert_eq!(display.blood_id(), 3);
    assert_eq!(display.npc_sound_id(), 4);
    assert_eq!(display.particle_color_id(), 5);
    assert_eq!(display.geoset_data(), 0x0012_0304);
    assert_eq!(display.object_effect_package_id(), 6);

    let extra = catalog
        .display_extra(display.extended_display_info_id())
        .ok_or("extended display row was not indexed")?;
    assert_eq!(extra.race_id(), 4);
    assert_eq!(extra.gender_id(), 1);
    assert_eq!(extra.skin_id(), 2);
    assert_eq!(extra.face_id(), 3);
    assert_eq!(extra.hair_style_id(), 4);
    assert_eq!(extra.hair_color_id(), 5);
    assert_eq!(extra.facial_hair_style_id(), 6);
    assert_eq!(
        extra.npc_item_display_ids(),
        [101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111]
    );
    assert_eq!(extra.flags(), 0x20);
    assert_eq!(extra.baked_texture_name(), "Textures\\BakedNpc.blp");

    let model = catalog
        .model(display.model_id())
        .ok_or("model row was not indexed")?;
    assert_eq!(model.flags(), 0x10);
    assert_eq!(
        model.model_path().ok_or("model path was absent")?.as_str(),
        "CREATURE\\BEAR\\BEAR.M2"
    );
    assert_eq!(model.size_class(), 2);
    assert_eq!(model.model_scale(), 0.75);
    assert_eq!(model.blood_id(), 3);
    assert_eq!(model.footprint_texture_id(), 9);
    assert_eq!(model.footprint_extent(), [1.5, 2.5]);
    assert_eq!(model.footprint_particle_scale(), 0.5);
    assert_eq!(model.foley_material_id(), 12);
    assert_eq!(model.footstep_shake_size(), 13);
    assert_eq!(model.death_thud_shake_size(), 14);
    assert_eq!(model.sound_id(), 15);
    assert_eq!(model.collision_extent(), [1.25, 2.25]);
    assert_eq!(model.mount_height(), 3.25);
    assert_eq!(model.geometry_box_min(), [-1.0, -2.0, -3.0]);
    assert_eq!(model.geometry_box_max(), [1.0, 2.0, 3.0]);
    assert_eq!(model.world_effect_scale(), 1.1);
    assert_eq!(model.attached_effect_scale(), 1.2);
    assert_eq!(model.missile_collision(), [1.3, 1.4, 1.5]);
    assert_eq!(catalog.display(99), None);
    assert_eq!(catalog.model(99), None);

    let appearance = catalog.resolve_model(display.id())?;
    assert_eq!(appearance.display(), display);
    assert_eq!(appearance.model(), model);
    assert_eq!(appearance.extra(), Some(extra));
    assert_eq!(
        appearance.model_path(),
        model.model_path().ok_or("model path was absent")?
    );
    assert_eq!(
        appearance
            .texture_for(M2TextureKind::Monster1)
            .map(AssetPath::as_str),
        Some("CREATURE\\BEAR\\BEARSKINBROWN.BLP")
    );
    assert_eq!(
        appearance
            .texture_for(M2TextureKind::Monster2)
            .map(AssetPath::as_str),
        Some("CREATURE\\BEAR\\BEARSKINBLACK.BLP")
    );
    assert_eq!(appearance.texture_for(M2TextureKind::Monster3), None);
    assert_eq!(appearance.texture_for(M2TextureKind::Hardcoded), None);
    Ok(())
}

/// Another client's layout is rejected rather than decoded through guessed offsets.
#[test]
fn creature_catalog_rejects_non_stock_layout() -> Result<(), Box<dyn Error>> {
    let display_table = create_wdbc(1, 15, &[0; 15], b"\0");
    let extra_table = create_wdbc(0, 21, &[], b"\0");
    let model_table = create_wdbc(0, 28, &[], b"\0");
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\CreatureDisplayInfo.dbc",
            bytes: &display_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\CreatureModelData.dbc",
            bytes: &model_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\CreatureDisplayInfoExtra.dbc",
            bytes: &extra_table,
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    assert!(matches!(
        CreatureCatalog::load(&mut store),
        Err(AssetError::DatabaseDecode { path, message })
            if path == AssetPath::new("DBFilesClient/CreatureDisplayInfo.dbc")?
                && message.contains("requires 16 fields")
    ));
    Ok(())
}

/// Player appearance bytes resolve to exact texture and geometry table keys.
#[test]
fn character_appearance_catalog_indexes_stock_customization_keys() -> Result<(), Box<dyn Error>> {
    let mut section_strings = vec![0];
    let upper = append_string(&mut section_strings, "Character\\NightElf\\Upper.blp");
    let lower = append_string(&mut section_strings, "Character\\NightElf\\Lower.blp");
    let sections = [
        2, 4, 1, 3, upper, lower, 0, 4, 7, 5, 1, 4, 1, 3, upper, lower, 0, 1, 7, 5,
    ];
    let section_table = create_wdbc(2, 10, &sections, &section_strings);
    let hair_table = create_wdbc(1, 6, &[90, 4, 1, 7, 12, 1], b"\0");
    let facial_table = create_wdbc(1, 8, &[4, 1, 6, 1, 2, 3, 4, 5], b"\0");
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\CharSections.dbc",
            bytes: &section_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\CharHairGeosets.dbc",
            bytes: &hair_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\CharacterFacialHairStyles.dbc",
            bytes: &facial_table,
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    let catalog = CharacterAppearanceCatalog::load(&mut store)?;
    let matching = catalog.sections_for(4, 1, 3, 7, 5);
    assert_eq!(matching.len(), 2);
    assert_eq!(matching[0].id(), 2);
    assert_eq!(matching[0].flags(), 4);
    assert_eq!(matching[1].id(), 1);
    assert_eq!(matching[1].flags(), 1);
    assert_eq!(matching[0].race_id(), 4);
    assert_eq!(matching[0].gender_id(), 1);
    assert_eq!(matching[0].base_section(), 3);
    assert_eq!(matching[0].variation_index(), 7);
    assert_eq!(matching[0].color_index(), 5);
    assert_eq!(
        matching[0].texture_names(),
        [
            "Character\\NightElf\\Upper.blp",
            "Character\\NightElf\\Lower.blp",
            ""
        ]
    );
    assert!(catalog.sections_for(4, 1, 3, 7, 6).is_empty());

    let hair = catalog
        .hair_geoset(4, 1, 7)
        .ok_or("hair geoset was not indexed")?;
    assert_eq!(hair.id(), 90);
    assert_eq!(hair.geoset_id(), 12);
    assert_eq!(hair.shows_scalp(), 1);
    assert_eq!(catalog.hair_geoset(4, 1, 8), None);

    let facial = catalog
        .facial_hair_style(4, 1, 6)
        .ok_or("facial-hair row was not indexed")?;
    assert_eq!(facial.geosets(), [1, 2, 3, 4, 5]);
    assert_eq!(catalog.facial_hair_style(4, 1, 7), None);
    Ok(())
}

/// Stock component keys produce the complete player texture and geoset plan.
#[test]
fn character_appearance_resolves_stock_component_keys() -> Result<(), Box<dyn Error>> {
    let mut strings = vec![0];
    let skin = append_string(&mut strings, "Character\\NightElf\\Female\\Skin.blp");
    let skin_extra = append_string(&mut strings, "Character\\NightElf\\Female\\SkinExtra.blp");
    let face_lower = append_string(&mut strings, "Character\\NightElf\\Female\\FaceLower.blp");
    let face_upper = append_string(&mut strings, "Character\\NightElf\\Female\\FaceUpper.blp");
    let facial_lower = append_string(&mut strings, "Character\\NightElf\\Female\\FacialLower.blp");
    let facial_upper = append_string(&mut strings, "Character\\NightElf\\Female\\FacialUpper.blp");
    let hair = append_string(&mut strings, "Character\\NightElf\\Female\\Hair.blp");
    let hair_lower = append_string(&mut strings, "Character\\NightElf\\Female\\HairLower.blp");
    let hair_upper = append_string(&mut strings, "Character\\NightElf\\Female\\HairUpper.blp");
    let underwear_lower = append_string(
        &mut strings,
        "Character\\NightElf\\Female\\UnderwearLower.blp",
    );
    let underwear_upper = append_string(
        &mut strings,
        "Character\\NightElf\\Female\\UnderwearUpper.blp",
    );
    let sections = [
        10,
        4,
        1,
        0,
        skin,
        skin_extra,
        0,
        17,
        0,
        2,
        11,
        4,
        1,
        1,
        face_lower,
        face_upper,
        0,
        1,
        3,
        2,
        12,
        4,
        1,
        2,
        facial_lower,
        facial_upper,
        0,
        1,
        6,
        5,
        13,
        4,
        1,
        3,
        hair,
        hair_lower,
        hair_upper,
        18,
        4,
        5,
        14,
        4,
        1,
        4,
        underwear_lower,
        underwear_upper,
        0,
        1,
        0,
        2,
    ];
    let section_table = create_wdbc(5, 10, &sections, &strings);
    // Zero is deliberately authored: stock converts it to geoset one.
    let hair_table = create_wdbc(1, 6, &[90, 4, 1, 4, 0, 1], b"\0");
    let facial_table = create_wdbc(1, 8, &[4, 1, 6, 1, 2, 3, 4, 5], b"\0");
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\CharSections.dbc",
            bytes: &section_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\CharHairGeosets.dbc",
            bytes: &hair_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\CharacterFacialHairStyles.dbc",
            bytes: &facial_table,
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let catalog = CharacterAppearanceCatalog::load(&mut store)?;

    let appearance = catalog.resolve_player(4, 1, CharacterCustomization::new(2, 3, 4, 5, 6))?;

    assert_eq!(appearance.skin().id(), 10);
    assert_eq!(appearance.face().map(|section| section.id()), Some(11));
    assert_eq!(appearance.facial_hair().id(), 12);
    assert_eq!(appearance.hair().id(), 13);
    assert_eq!(appearance.underwear().map(|section| section.id()), Some(14));
    assert_eq!(appearance.hair_geoset().map(|row| row.id()), Some(90));
    assert!(appearance.facial_hair_style().is_some());
    assert_eq!(appearance.geosets().hair(), 1);
    assert_eq!(
        appearance.geosets().facial_hair(),
        Some([101, 203, 302, 1604, 1705])
    );
    Ok(())
}

/// Missing customization rows remain a typed failure instead of selecting a neighbor.
#[test]
fn character_appearance_does_not_fallback_to_another_color() -> Result<(), Box<dyn Error>> {
    let mut strings = vec![0];
    let skin = append_string(&mut strings, "Character\\Human\\Male\\Skin.blp");
    let section_table = create_wdbc(1, 10, &[1, 1, 0, 0, skin, 0, 0, 17, 0, 0], &strings);
    let hair_table = create_wdbc(0, 6, &[], b"\0");
    let facial_table = create_wdbc(0, 8, &[], b"\0");
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\CharSections.dbc",
            bytes: &section_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\CharHairGeosets.dbc",
            bytes: &hair_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\CharacterFacialHairStyles.dbc",
            bytes: &facial_table,
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let catalog = CharacterAppearanceCatalog::load(&mut store)?;

    assert_eq!(
        catalog
            .resolve_player(1, 0, CharacterCustomization::new(1, 0, 0, 0, 0))
            .err(),
        Some(AppearanceError::MissingCharacterSection {
            race_id: 1,
            gender_id: 0,
            kind: solarity_asset::CharacterSectionKind::Skin,
            variation_index: 0,
            color_index: 1,
        })
    );
    Ok(())
}

/// Packed later-client customization schemas are not accepted as build 12340.
#[test]
fn character_appearance_catalog_rejects_non_stock_layout() -> Result<(), Box<dyn Error>> {
    let section_table = create_wdbc(0, 9, &[], b"\0");
    let hair_table = create_wdbc(0, 6, &[], b"\0");
    let facial_table = create_wdbc(0, 8, &[], b"\0");
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\CharSections.dbc",
            bytes: &section_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\CharHairGeosets.dbc",
            bytes: &hair_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\CharacterFacialHairStyles.dbc",
            bytes: &facial_table,
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    assert!(matches!(
        CharacterAppearanceCatalog::load(&mut store),
        Err(AssetError::DatabaseDecode { path, message })
            if path == AssetPath::new("DBFilesClient/CharSections.dbc")?
                && message.contains("requires 10 fields")
    ));
    Ok(())
}

/// Build-12340 item displays retain model, geoset, and component texture fields.
#[test]
fn item_display_catalog_decodes_stock_equipment_layout() -> Result<(), Box<dyn Error>> {
    let mut strings = vec![0];
    let model_left = append_string(&mut strings, "Helm_Plate_Raid_LichKing_C_01.m2");
    let model_right = append_string(&mut strings, "Helm_Plate_Raid_LichKing_C_01_Visor.m2");
    let texture_left = append_string(&mut strings, "Helm_Plate_RaidLichKing_C_01Blue");
    let texture_right = append_string(&mut strings, "Helm_Plate_RaidLichKing_C_01Blue_Visor");
    let icon_left = append_string(&mut strings, "INV_Helmet_170");
    let icon_right = append_string(&mut strings, "INV_Helmet_170_2");
    let arm_upper = append_string(&mut strings, "Plate_RaidLichKing_C_01Blue_ArmUpper");
    let arm_lower = append_string(&mut strings, "Plate_RaidLichKing_C_01Blue_ArmLower");
    let hand = append_string(&mut strings, "Plate_RaidLichKing_C_01Blue_Hand");
    let torso_upper = append_string(&mut strings, "Plate_RaidLichKing_C_01Blue_TorsoUpper");
    let torso_lower = append_string(&mut strings, "Plate_RaidLichKing_C_01Blue_TorsoLower");
    let leg_upper = append_string(&mut strings, "Plate_RaidLichKing_C_01Blue_LegUpper");
    let leg_lower = append_string(&mut strings, "Plate_RaidLichKing_C_01Blue_LegLower");
    let foot = append_string(&mut strings, "Plate_RaidLichKing_C_01Blue_Foot");
    let fields = [
        55_000,
        model_left,
        model_right,
        texture_left,
        texture_right,
        icon_left,
        icon_right,
        2,
        3,
        4,
        0x40,
        71,
        9,
        101,
        102,
        arm_upper,
        arm_lower,
        hand,
        torso_upper,
        torso_lower,
        leg_upper,
        leg_lower,
        foot,
        88,
        99,
    ];
    let table = create_wdbc(1, 25, &fields, &strings);
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\ItemDisplayInfo.dbc",
        bytes: &table,
    }])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    let catalog = ItemDisplayCatalog::load(&mut store)?;
    let display = catalog.display(55_000).ok_or("item display is absent")?;

    assert_eq!(
        display.model_names(),
        [
            "Helm_Plate_Raid_LichKing_C_01.m2",
            "Helm_Plate_Raid_LichKing_C_01_Visor.m2"
        ]
    );
    assert_eq!(display.geoset_groups(), [2, 3, 4]);
    assert_eq!(display.flags(), 0x40);
    assert_eq!(display.helmet_geoset_visibility_ids(), [101, 102]);
    assert_eq!(
        display.component_textures(),
        [
            "Plate_RaidLichKing_C_01Blue_ArmUpper",
            "Plate_RaidLichKing_C_01Blue_ArmLower",
            "Plate_RaidLichKing_C_01Blue_Hand",
            "Plate_RaidLichKing_C_01Blue_TorsoUpper",
            "Plate_RaidLichKing_C_01Blue_TorsoLower",
            "Plate_RaidLichKing_C_01Blue_LegUpper",
            "Plate_RaidLichKing_C_01Blue_LegLower",
            "Plate_RaidLichKing_C_01Blue_Foot",
        ]
    );
    assert_eq!(catalog.display(55_001), None);
    Ok(())
}

/// Client item entries resolve display and equipment categories without server data.
#[test]
fn item_definition_catalog_decodes_visible_item_lookup() -> Result<(), Box<dyn Error>> {
    let table = create_wdbc(
        2,
        8,
        &[
            50_001,
            4,
            4,
            u32::MAX,
            1,
            55_000,
            5,
            0,
            50_002,
            2,
            7,
            7,
            u32::MAX,
            55_001,
            17,
            1,
        ],
        b"\0",
    );
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\Item.dbc",
        bytes: &table,
    }])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    let catalog = ItemDefinitionCatalog::load(&mut store)?;
    let chest = catalog.item(50_001).ok_or("item definition is absent")?;
    assert_eq!(chest.display_info_id(), 55_000);
    assert_eq!(chest.inventory_type(), InventoryType::Chest);
    assert_eq!(chest.sound_override_subclass_id(), -1);
    assert_eq!(chest.material_id(), 1);
    let weapon = catalog.item(50_002).ok_or("weapon definition is absent")?;
    assert_eq!(weapon.inventory_type(), InventoryType::TwoHandWeapon);
    assert_eq!(weapon.sheathe_type(), 1);
    assert_eq!(catalog.item(50_003), None);
    Ok(())
}

/// Helmet visibility rows preserve all seven build-12340 mask words.
#[test]
fn helmet_visibility_catalog_decodes_exact_masks() -> Result<(), Box<dyn Error>> {
    let table = create_wdbc(
        2,
        8,
        &[
            101, 0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 102, 0x80, 0x100, 0x200, 0x400, 0x800,
            0x1000, 0x2000,
        ],
        b"\0",
    );
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\HelmetGeosetVisData.dbc",
        bytes: &table,
    }])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    let catalog = HelmetGeosetVisibilityCatalog::load(&mut store)?;
    let visibility = catalog.visibility(101).ok_or("helmet row is absent")?;

    assert_eq!(visibility.hair_flags(), 0x01);
    assert_eq!(visibility.facial_hair_flags(), [0x02, 0x04, 0x08]);
    assert_eq!(visibility.ear_flags(), 0x10);
    assert_eq!(visibility.additional_flags(), [0x20, 0x40]);
    assert_eq!(catalog.visibility(103), None);
    Ok(())
}

/// Character race rows retain stock helmet and body-model filename components.
#[test]
fn character_race_catalog_decodes_model_naming_fields() -> Result<(), Box<dyn Error>> {
    let mut strings = vec![0];
    let prefix = append_string(&mut strings, "Hu");
    let file_string = append_string(&mut strings, "Human");
    let display_name = append_string(&mut strings, "Human");
    let mut fields = [0_u32; 69];
    fields[0] = 1;
    fields[1] = 0x0080_0001;
    fields[4] = 49;
    fields[5] = 50;
    fields[6] = prefix;
    fields[11] = file_string;
    fields[14] = display_name;
    let table = create_wdbc(1, 69, &fields, &strings);
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\ChrRaces.dbc",
        bytes: &table,
    }])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    let catalog = CharacterRaceCatalog::load(&mut store)?;
    let race = catalog.race(1).ok_or("character race is absent")?;
    assert_eq!(race.flags(), 0x0080_0001);
    assert_eq!(race.male_display_id(), 49);
    assert_eq!(race.female_display_id(), 50);
    assert_eq!(race.client_prefix(), "Hu");
    assert_eq!(race.client_file_string(), "Human");
    assert_eq!(race.name(), "Human");
    assert_eq!(catalog.race(2), None);
    Ok(())
}

/// Character-selection labels come from each table's exact locale slot.
#[test]
fn character_selection_catalogs_decode_localized_labels() -> Result<(), Box<dyn Error>> {
    let mut class_strings = vec![0];
    let class_name = append_string(&mut class_strings, "Mage");
    let mut class_fields = [0_u32; 60];
    class_fields[0] = 8;
    class_fields[4] = class_name;
    let classes = create_wdbc(1, 60, &class_fields, &class_strings);

    let mut area_strings = vec![0];
    let area_name = append_string(&mut area_strings, "Dalaran");
    let mut area_fields = [0_u32; 36];
    area_fields[0] = 4395;
    area_fields[1] = 571;
    area_fields[11] = area_name;
    let areas = create_wdbc(1, 36, &area_fields, &area_strings);
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\ChrClasses.dbc",
            bytes: &classes,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\AreaTable.dbc",
            bytes: &areas,
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    let classes = CharacterClassCatalog::load(&mut store)?;
    assert_eq!(classes.class(8).map(|class| class.name()), Some("Mage"));
    assert_eq!(classes.class(9), None);
    let areas = AreaTableCatalog::load(&mut store)?;
    let area = areas.area(4395).ok_or("Dalaran area is absent")?;
    assert_eq!(area.name(), "Dalaran");
    assert_eq!(area.parent_area_id(), 0);
    assert_eq!(areas.area(4396), None);
    Ok(())
}

/// World asset resolution uses the exact internal directory from `Map.dbc`.
#[test]
fn map_catalog_decodes_build_12340_world_identity() -> Result<(), Box<dyn Error>> {
    let mut strings = vec![0];
    let directory = append_string(&mut strings, "Northrend");
    let name = append_string(&mut strings, "Northrend");
    let mut fields = [0_u32; 66];
    fields[0] = 571;
    fields[1] = directory;
    fields[2] = 0;
    fields[3] = 0x20;
    fields[5] = name;
    fields[22] = 571;
    fields[59] = u32::MAX;
    fields[60] = 0.0_f32.to_bits();
    fields[61] = 0.0_f32.to_bits();
    fields[63] = 2;
    fields[65] = 0;
    let table = create_wdbc(1, 66, &fields, &strings);
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\Map.dbc",
        bytes: &table,
    }])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    let catalog = MapCatalog::load(&mut store)?;
    let map = catalog.map(571).ok_or("Northrend map is absent")?;
    assert_eq!(map.directory(), "Northrend");
    assert_eq!(map.kind(), MapKind::World);
    assert_eq!(map.flags(), 0x20);
    assert_eq!(map.name(), "Northrend");
    assert_eq!(map.linked_zone_id(), 571);
    assert_eq!(map.entrance(), None);
    assert_eq!(map.expansion_id(), 2);
    assert_eq!(map.maximum_players(), 0);
    assert_eq!(catalog.map(572), None);
    Ok(())
}

/// Generates the fixed WDBC layout used by stock-era client tables.
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

/// Adds one NUL-terminated fixture string and returns its WDBC offset.
fn append_string(block: &mut Vec<u8>, value: &str) -> u32 {
    let offset = block.len() as u32;
    block.extend_from_slice(value.as_bytes());
    block.push(0);
    offset
}
