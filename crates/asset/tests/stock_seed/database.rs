//! External stock-compatibility tests for WDBC bootstrap.

use std::error::Error;
use std::path::Path;

use solarity_asset::{
    AdvancedSoundEntryCatalog, AnimationDataCatalog, AppearanceError, ArchiveCatalog,
    AreaTableCatalog, AssetError, AssetPath, AssetStore, CharacterAppearanceCatalog,
    CharacterBaseCatalog, CharacterClassCatalog, CharacterCustomization, CharacterFactionCatalog,
    CharacterRaceCatalog, ClientDataRoot, CreatureCatalog, HelmetGeosetVisibilityCatalog,
    InventoryType, ItemDefinitionCatalog, ItemDisplayCatalog, ItemVisualCatalog, LightCatalog,
    Locale, M2TextureKind, MapCatalog, MapDifficultyCatalog, MapKind, PaperDollItemFrameCatalog,
    ParticleColorCatalog, SoundEntryCatalog, WdbcTable, WorldLightQuery, WorldLightSampleError,
    exterior_light_direction,
};

use crate::support::{Fixture, FixtureFile};

/// 0x00634950 searches one map's contiguous run for an exact difficulty;
/// 0x00403910 distinguishes an empty authored message from absent map data.
#[test]
fn map_difficulty_messages_preserve_exact_matches_and_empty_text() -> Result<(), Box<dyn Error>> {
    let mut rows = Vec::new();
    for (id, map, difficulty, message) in [
        (1, 530, 0, 1),
        (2, 530, 1, 0),
        (3, 571, 0, 7),
        (4, 530, 2, 1),
    ] {
        let mut row = [0_u32; 23];
        row[..4].copy_from_slice(&[id, map, difficulty, message]);
        rows.extend_from_slice(&row);
    }
    let table = create_wdbc(4, 23, &rows, b"\0first\0second\0");
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\MapDifficulty.dbc",
        bytes: &table,
    }])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let difficulties = MapDifficultyCatalog::load(&mut store)?;
    assert_eq!(difficulties.message(530, 0), Some("first"));
    assert_eq!(difficulties.message(530, 1), Some(""));
    assert_eq!(difficulties.message(571, 0), Some("second"));
    assert_eq!(difficulties.message(530, 2), None);
    assert_eq!(difficulties.message(571, 1), None);
    assert_eq!(difficulties.message(999, 0), None);
    Ok(())
}

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

/// Paper-doll inventory metadata retains DBC-authored names, icons, and slots.
#[test]
fn paper_doll_item_frame_catalog_decodes_exact_stock_layout() -> Result<(), Box<dyn Error>> {
    let mut strings = vec![0];
    let head_name = append_string(&mut strings, "HeadSlot");
    let head_icon = append_string(&mut strings, "Interface\\PaperDoll\\UI-PaperDoll-Slot-Head");
    let ranged_name = append_string(&mut strings, "RangedSlot");
    let ranged_icon = append_string(
        &mut strings,
        "Interface\\PaperDoll\\UI-PaperDoll-Slot-Ranged",
    );
    let table = create_wdbc(
        2,
        3,
        &[head_name, head_icon, 1, ranged_name, ranged_icon, 18],
        &strings,
    );
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\PaperDollItemFrame.dbc",
        bytes: &table,
    }])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    let catalog = PaperDollItemFrameCatalog::load(&mut store)?;
    assert_eq!(catalog.definitions().len(), 2);
    let head = catalog
        .definition("HEADSLOT")
        .ok_or("head slot is absent")?;
    assert_eq!(head.item_button_name(), "HeadSlot");
    assert_eq!(
        head.slot_icon(),
        "Interface\\PaperDoll\\UI-PaperDoll-Slot-Head"
    );
    assert_eq!(head.slot_number(), 1);
    assert_eq!(
        catalog
            .definition("rangedslot")
            .ok_or("ranged slot is absent")?
            .slot_number(),
        18
    );
    assert!(catalog.definition("MissingSlot").is_none());
    Ok(())
}

/// Particle replacement ramps preserve all three stock color triplets.
#[test]
fn particle_color_catalog_decodes_exact_stock_layout() -> Result<(), Box<dyn Error>> {
    let fields = [
        17,
        0x0011_2233,
        0x0044_5566,
        0x0077_8899,
        0x00aa_bbcc,
        0x00dd_eeff,
        0x0001_0203,
        0x0004_0506,
        0x0007_0809,
        0x000a_0b0c,
    ];
    let table = create_wdbc(1, 10, &fields, b"\0");
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\ParticleColor.dbc",
        bytes: &table,
    }])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    let catalog = ParticleColorCatalog::load(&mut store)?;
    let definition = catalog.definition(17).ok_or("particle color is absent")?;
    assert_eq!(definition.id(), 17);
    assert_eq!(definition.start(), fields[1..4]);
    assert_eq!(definition.middle(), fields[4..7]);
    assert_eq!(definition.end(), fields[7..10]);
    assert!(catalog.definition(18).is_none());
    Ok(())
}

/// Another ParticleColor layout cannot be reinterpreted as build 12340.
#[test]
fn particle_color_catalog_rejects_non_stock_layout() -> Result<(), Box<dyn Error>> {
    let table = create_wdbc(1, 9, &[0; 9], b"\0");
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\ParticleColor.dbc",
        bytes: &table,
    }])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    assert!(matches!(
        ParticleColorCatalog::load(&mut store),
        Err(AssetError::DatabaseDecode { .. })
    ));
    Ok(())
}

/// AnimationData retains flags, fallback links, and behavior-tier families.
#[test]
fn animation_data_catalog_decodes_and_maps_tiered_behaviors() -> Result<(), Box<dyn Error>> {
    let mut strings = vec![0];
    let stand = append_string(&mut strings, "Stand");
    let walk = append_string(&mut strings, "Walk");
    let fly_walk = append_string(&mut strings, "FlyWalk");
    let fields = [
        0, stand, 0x10, 0x20, 0x80, 0, 0, 0, 4, walk, 1, 2, 3, 0, 4, 0, 233, fly_walk, 5, 6, 7, 4,
        4, 3,
    ];
    let table = create_wdbc(3, 8, &fields, &strings);
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\AnimationData.dbc",
        bytes: &table,
    }])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    let catalog = AnimationDataCatalog::load(&mut store)?;
    let definition = catalog.definition(4).ok_or("walk animation is absent")?;
    assert_eq!(definition.name(), "Walk");
    assert_eq!(definition.weapon_flags(), 1);
    assert_eq!(definition.body_flags(), 2);
    assert_eq!(definition.flags(), 3);
    assert_eq!(definition.fallback_id(), 0);
    assert_eq!(definition.behavior_id(), 4);
    assert_eq!(definition.behavior_tier(), 0);
    assert_eq!(catalog.tiered_definition(4, 0), Some(definition));
    assert_eq!(
        catalog.tiered_definition(4, 3).map(|row| row.id()),
        Some(233)
    );
    assert_eq!(
        catalog.tiered_definition(233, 0).map(|row| row.id()),
        Some(233)
    );
    assert_eq!(catalog.definitions().len(), 3);
    Ok(())
}

/// Another AnimationData layout cannot be interpreted as build 12340.
#[test]
fn animation_data_catalog_rejects_non_stock_layout() -> Result<(), Box<dyn Error>> {
    let table = create_wdbc(1, 7, &[0; 7], b"\0");
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\AnimationData.dbc",
        bytes: &table,
    }])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    assert!(matches!(
        AnimationDataCatalog::load(&mut store),
        Err(AssetError::DatabaseDecode { .. })
    ));
    Ok(())
}

/// Exact build-12340 creature tables resolve update-field display IDs to M2 metadata.
#[test]
fn creature_catalog_decodes_stock_display_and_model_layouts() -> Result<(), Box<dyn Error>> {
    let mut display_strings = vec![0];
    let skin_1 = append_string(&mut display_strings, "BearSkinBrown.blp");
    let skin_2 = append_string(&mut display_strings, "BearSkinBlack");
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
    let baked_texture = append_string(&mut extra_strings, "ABCDEF0123456789");
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
        ["BearSkinBrown.blp", "BearSkinBlack", ""]
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
    assert_eq!(extra.baked_texture_name(), "ABCDEF0123456789");

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
        appearance.baked_texture().map(AssetPath::as_str),
        Some("TEXTURES\\BAKEDNPCTEXTURES\\ABCDEF0123456789.BLP")
    );
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
    assert_eq!(facial.race_id(), 4);
    assert_eq!(facial.gender_id(), 1);
    assert_eq!(facial.variation_id(), 6);
    assert_eq!(facial.geosets(), [1, 2, 3, 4, 5]);
    assert_eq!(catalog.facial_hair_style(4, 1, 7), None);
    // The fixture deliberately has no facial-hair CharSections row. Stock's
    // selectable feature list comes from CharacterFacialHairStyles instead.
    assert_eq!(catalog.player_facial_hair_styles(4, 1), vec![6]);
    Ok(())
}

/// Build 12340 retains duplicate hair keys and selects the last physical row.
#[test]
fn character_hair_geoset_lookup_uses_stock_last_match_semantics() -> Result<(), Box<dyn Error>> {
    let section_table = create_wdbc(0, 10, &[], b"\0");
    let hair_table = create_wdbc(
        3,
        6,
        &[
            90, 8, 0, 0, 4, 0, // Earlier matching row.
            91, 1, 0, 0, 2, 1, // Intervening key proves sorting is stable.
            92, 8, 0, 0, 7, 1, // Stock-visible last matching row.
        ],
        b"\0",
    );
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
    let selected = catalog
        .hair_geoset(8, 0, 0)
        .ok_or("duplicate stock hair key was not retained")?;
    assert_eq!(selected.id(), 92);
    assert_eq!(selected.geoset_id(), 7);
    assert_eq!(selected.shows_scalp(), 1);
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
    let facial_table = create_wdbc(
        2,
        8,
        &[4, 1, 0, 0, 0, 0, 0, 0, 4, 1, 6, 1, 2, 3, 4, 5],
        b"\0",
    );
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
    assert_eq!(
        appearance.facial_hair().map(|section| section.id()),
        Some(12)
    );
    assert_eq!(appearance.hair().map(|section| section.id()), Some(13));
    assert_eq!(appearance.underwear().map(|section| section.id()), Some(14));
    assert_eq!(appearance.hair_geoset().map(|row| row.id()), Some(90));
    assert!(appearance.facial_hair_style().is_some());
    assert_eq!(appearance.geosets().hair(), 1);
    assert_eq!(
        appearance.geosets().facial_hair(),
        Some([101, 203, 302, 1604, 1705])
    );

    let geometry_only = catalog.resolve_player(4, 1, CharacterCustomization::new(2, 3, 4, 5, 0))?;
    assert_eq!(geometry_only.facial_hair(), None);
    assert!(geometry_only.facial_hair_style().is_some());
    assert_eq!(
        geometry_only.geosets().facial_hair(),
        Some([100, 200, 300, 1600, 1700])
    );
    Ok(())
}

/// Baked NPC skins can omit hair without selecting an unrelated section.
#[test]
fn character_appearance_keeps_hairless_npc_sections_absent() -> Result<(), Box<dyn Error>> {
    let sections = create_wdbc(
        1,
        10,
        &[1, 9, 0, 0, 1, 0, 0, 8, 0, 0],
        b"\0GoblinSkin.blp\0",
    );
    let hair = create_wdbc(0, 6, &[], b"\0");
    let facial = create_wdbc(0, 8, &[], b"\0");
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\CharSections.dbc",
            bytes: &sections,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\CharHairGeosets.dbc",
            bytes: &hair,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\CharacterFacialHairStyles.dbc",
            bytes: &facial,
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let catalog = CharacterAppearanceCatalog::load(&mut store)?;
    let appearance = catalog.resolve_player(9, 0, CharacterCustomization::new(0, 0, 0, 0, 0))?;
    assert_eq!(appearance.skin().texture_names()[0], "GoblinSkin.blp");
    assert!(appearance.hair().is_none());
    assert!(appearance.face().is_none());
    assert!(appearance.underwear().is_none());
    assert_eq!(appearance.geosets().hair(), 1);
    Ok(())
}

/// Missing skin colors remain a typed failure instead of selecting a neighbor.
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

/// Item visuals retain five attachment slots and join public enchantments.
#[test]
fn item_visual_catalog_decodes_stock_effect_stack() -> Result<(), Box<dyn Error>> {
    let mut strings = vec![0];
    let blue_glow = append_string(&mut strings, "Spells\\Enchantments\\BlueGlow_High.mdx");
    let empty_effect = append_string(&mut strings, "Spells\\Enchantments\\");
    let visuals = create_wdbc(1, 6, &[24, 1, 1, 0xFFFF_FFFF, 61, 0], &[0]);
    let effects = create_wdbc(2, 2, &[1, blue_glow, 61, empty_effect], &strings);
    let mut enchantment = [0_u32; 38];
    enchantment[0] = 1_896;
    enchantment[31] = 24;
    let enchantments = create_wdbc(1, 38, &enchantment, &[0]);
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\ItemVisuals.dbc",
            bytes: &visuals,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\ItemVisualEffects.dbc",
            bytes: &effects,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\SpellItemEnchantment.dbc",
            bytes: &enchantments,
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    let catalog = ItemVisualCatalog::load(&mut store)?;
    assert_eq!(
        catalog
            .visual(24)
            .ok_or("item visual is absent")?
            .effect_ids(),
        [1, 1, u32::MAX, 61, 0]
    );
    assert_eq!(
        catalog
            .effect(1)
            .and_then(|effect| effect.model_path())
            .map(AssetPath::as_str),
        Some("SPELLS\\ENCHANTMENTS\\BLUEGLOW_HIGH.MDX")
    );
    assert_eq!(
        catalog.effect(61).and_then(|effect| effect.model_path()),
        None
    );
    assert_eq!(catalog.effect(u32::MAX), None);
    assert_eq!(
        catalog
            .enchantment(1_896)
            .ok_or("enchantment is absent")?
            .item_visual_id(),
        24
    );
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
    let female_name = append_string(&mut strings, "Human Woman");
    let male_name = append_string(&mut strings, "Human Man");
    let facial_male = append_string(&mut strings, "NORMAL");
    let facial_female = append_string(&mut strings, "NONE");
    let hair = append_string(&mut strings, "NORMAL");
    let mut fields = [0_u32; 69];
    fields[0] = 1;
    fields[1] = 0x0080_0001;
    fields[2] = 1;
    fields[4] = 49;
    fields[5] = 50;
    fields[6] = prefix;
    fields[11] = file_string;
    fields[14] = display_name;
    fields[31] = female_name;
    fields[48] = male_name;
    fields[65] = facial_male;
    fields[66] = facial_female;
    fields[67] = hair;
    fields[68] = 2;
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
    assert_eq!(race.faction_id(), 1);
    assert_eq!(race.male_display_id(), 49);
    assert_eq!(race.female_display_id(), 50);
    assert_eq!(race.client_prefix(), "Hu");
    assert_eq!(race.client_file_string(), "Human");
    assert_eq!(race.name(), "Human");
    assert_eq!(race.display_name(0), "Human Man");
    assert_eq!(race.display_name(1), "Human Woman");
    assert_eq!(race.facial_hair_customization(0), Some("NORMAL"));
    assert_eq!(race.facial_hair_customization(1), Some("NONE"));
    assert_eq!(race.hair_customization(), "NORMAL");
    assert_eq!(race.required_expansion(), 2);
    assert_eq!(catalog.races().count(), 1);
    assert_eq!(catalog.race(2), None);
    Ok(())
}

/// Character-selection labels come from each table's exact locale slot.
#[test]
fn character_selection_catalogs_decode_localized_labels() -> Result<(), Box<dyn Error>> {
    let mut class_strings = vec![0];
    let class_name = append_string(&mut class_strings, "Mage");
    let female_class_name = append_string(&mut class_strings, "Sorceress");
    let male_class_name = append_string(&mut class_strings, "Sorcerer");
    let class_file = append_string(&mut class_strings, "Mage");
    let mut class_fields = [0_u32; 60];
    class_fields[0] = 8;
    class_fields[4] = class_name;
    class_fields[21] = female_class_name;
    class_fields[38] = male_class_name;
    class_fields[55] = class_file;
    class_fields[59] = 2;
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
    let mage = classes.class(8).ok_or("Mage class is absent")?;
    assert_eq!(mage.female_name(), "Sorceress");
    assert_eq!(mage.male_name(), "Sorcerer");
    assert_eq!(mage.file_string(), "Mage");
    assert_eq!(mage.required_expansion(), 2);
    assert_eq!(classes.classes().count(), 1);
    assert_eq!(classes.class(9), None);
    let areas = AreaTableCatalog::load(&mut store)?;
    let area = areas.area(4395).ok_or("Dalaran area is absent")?;
    assert_eq!(area.name(), "Dalaran");
    assert_eq!(area.parent_area_id(), 0);
    assert_eq!(areas.area(4396), None);
    Ok(())
}

/// Packed `CharBaseInfo` rows retain physical class order and exact pairs.
#[test]
fn character_base_catalog_decodes_two_byte_rows() -> Result<(), Box<dyn Error>> {
    let table = create_packed_wdbc(3, 2, 2, &[1, 1, 1, 8, 2, 1], b"\0");
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\CharBaseInfo.dbc",
        bytes: &table,
    }])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;

    let catalog = CharacterBaseCatalog::load(&mut store)?;
    let pairs = catalog
        .entries()
        .iter()
        .map(|entry| (entry.race_id(), entry.class_id()))
        .collect::<Vec<_>>();
    assert_eq!(pairs, [(1, 1), (1, 8), (2, 1)]);
    assert!(catalog.supports(1, 8));
    assert!(!catalog.supports(2, 8));
    Ok(())
}

/// Character faction lookup follows the first physical matching group bit.
#[test]
fn character_faction_catalog_projects_template_groups() -> Result<(), Box<dyn Error>> {
    let templates = create_wdbc(
        2,
        14,
        &[
            1,
            0,
            0,
            1 << 1,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            2,
            0,
            0,
            1 << 2,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ],
        b"\0",
    );
    let mut group_strings = vec![0];
    let alliance_internal = append_string(&mut group_strings, "Alliance");
    let alliance_name = append_string(&mut group_strings, "Alliance");
    let horde_internal = append_string(&mut group_strings, "Horde");
    let horde_name = append_string(&mut group_strings, "Horde");
    let mut group_fields = vec![0_u32; 40];
    group_fields[0] = 1;
    group_fields[1] = 1;
    group_fields[2] = alliance_internal;
    group_fields[3] = alliance_name;
    group_fields[20] = 2;
    group_fields[21] = 2;
    group_fields[22] = horde_internal;
    group_fields[23] = horde_name;
    let groups = create_wdbc(2, 20, &group_fields, &group_strings);
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\FactionTemplate.dbc",
            bytes: &templates,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\FactionGroup.dbc",
            bytes: &groups,
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;

    let catalog = CharacterFactionCatalog::load(&mut store)?;
    let alliance = catalog.group_for_template(1).ok_or("Alliance is absent")?;
    let horde = catalog.group_for_template(2).ok_or("Horde is absent")?;
    assert_eq!(alliance.internal_name(), "Alliance");
    assert_eq!(alliance.name(), "Alliance");
    assert_eq!(horde.internal_name(), "Horde");
    assert_eq!(catalog.group_for_template(3), None);
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

/// Five exact light tables produce one shared exterior environment snapshot.
#[test]
fn light_catalog_samples_stock_color_and_float_channels() -> Result<(), Box<dyn Error>> {
    let light_fields = [1, 571, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0];
    let light_table = create_wdbc(1, 15, &light_fields, &[0]);
    let parameter_fields = [
        1,
        1,
        0,
        0.75_f32.to_bits(),
        0.1_f32.to_bits(),
        0.2_f32.to_bits(),
        0.3_f32.to_bits(),
        0.4_f32.to_bits(),
        0x20,
    ];
    let parameter_table = create_wdbc(1, 9, &parameter_fields, &[0]);
    let skybox_table = create_wdbc(0, 3, &[], &[0]);
    let color_table = constant_light_color_bands();
    let float_table = constant_light_float_bands();
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\Light.dbc",
            bytes: &light_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\LightParams.dbc",
            bytes: &parameter_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\LightSkybox.dbc",
            bytes: &skybox_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\LightIntBand.dbc",
            bytes: &color_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\LightFloatBand.dbc",
            bytes: &float_table,
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    let catalog = LightCatalog::load(&mut store)?;
    let sample = catalog.sample(WorldLightQuery::new(571, glam::Vec3::ZERO, 720))?;

    assert_eq!(catalog.lights().len(), 1);
    assert!(catalog.lights()[0].is_global());
    assert!(
        sample
            .diffuse_color()
            .abs_diff_eq(glam::Vec3::splat(128.0 / 255.0), 0.000_001,)
    );
    assert!(sample.ambient_color().abs_diff_eq(
        glam::Vec3::new(16.0 / 255.0, 32.0 / 255.0, 48.0 / 255.0),
        0.000_001,
    ));
    assert_eq!(sample.fog_range(), (50.0, 100.0));
    assert_eq!(sample.highlight_sky(), 1.0);
    assert_eq!(sample.glow(), 0.75);
    assert_eq!(sample.sky_floats(), [0.25, 0.75, 1.25, 1.5]);
    assert_eq!(sample.liquid_alphas(), [0.3, 0.4, 0.1, 0.2]);
    assert_eq!(
        catalog.sample(WorldLightQuery::new(0, glam::Vec3::ZERO, 720)),
        Err(WorldLightSampleError::MissingGlobalLight {
            map_id: 0,
            condition: 0,
        })
    );
    Ok(())
}

/// Glue can sample parameter three without a world volume or unused bands.
#[test]
fn model_light_colors_sample_direct_parameter_channels() -> Result<(), Box<dyn Error>> {
    let light_table = create_wdbc(0, 15, &[], &[0]);
    let parameter_table = create_wdbc(1, 9, &[3, 0, 0, 0, 0, 0, 0, 0, 0], &[0]);
    let skybox_table = create_wdbc(0, 3, &[], &[0]);
    let float_table = create_wdbc(0, 34, &[], &[0]);
    let mut colors = [0_u32; 68];
    for (row, start, end) in [(0, 0x1e2832, 0x78828c), (1, 0x46505a, 0xa0aab4)] {
        colors[row * 34] = 37 + row as u32;
        colors[row * 34 + 1] = 2;
        colors[row * 34 + 3] = 1_440;
        colors[row * 34 + 18] = start;
        colors[row * 34 + 19] = end;
    }
    let color_table = create_wdbc(2, 34, &colors, &[0]);
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\Light.dbc",
            bytes: &light_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\LightParams.dbc",
            bytes: &parameter_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\LightSkybox.dbc",
            bytes: &skybox_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\LightIntBand.dbc",
            bytes: &color_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\LightFloatBand.dbc",
            bytes: &float_table,
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let catalog = LightCatalog::load(&mut store)?;
    let midnight = catalog.model_light_colors(3, 0)?;
    assert_eq!(
        midnight.diffuse(),
        glam::Vec3::new(30.0, 40.0, 50.0) / 255.0
    );
    assert_eq!(
        midnight.ambient(),
        glam::Vec3::new(70.0, 80.0, 90.0) / 255.0
    );
    let dawn = catalog.model_light_colors(3, 720)?;
    assert_eq!(dawn.diffuse(), glam::Vec3::new(75.0, 85.0, 95.0) / 255.0);
    assert_eq!(dawn.ambient(), glam::Vec3::new(115.0, 125.0, 135.0) / 255.0);
    assert_eq!(catalog.model_light_colors(3, 2_880)?, midnight);
    assert_eq!(
        catalog.model_light_colors(4, 0),
        Err(WorldLightSampleError::MissingParameterId { parameter_id: 4 }),
    );
    Ok(())
}

/// The native cubic day/night path wraps and retains recovered key vectors.
#[test]
fn exterior_light_direction_uses_the_executable_table() {
    let midnight = exterior_light_direction(0);
    let dawn = exterior_light_direction(720);

    assert!((midnight.length() - 1.0).abs() < 0.000_01);
    assert!((dawn.length() - 1.0).abs() < 0.000_01);
    assert!(midnight.abs_diff_eq(glam::Vec3::new(0.561_309, 0.561_309, 0.608_165), 0.000_02,));
    assert!((dawn.x - 0.664_877).abs() < 0.000_02);
    assert!((dawn.z - 0.340_406).abs() < 0.000_02);
    assert_eq!(exterior_light_direction(1_440), midnight);
    assert_eq!(exterior_light_direction(2_880), midnight);
}

/// Builds all 18 one-key packed-color rows for LightParams ID one.
fn constant_light_color_bands() -> Vec<u8> {
    let mut fields = Vec::with_capacity(18 * 34);
    for id in 1..=18_u32 {
        let color = match id {
            2 => 0x0010_2030,
            8 => 0x0040_5060,
            10 => 0x0070_8090,
            _ => id * 0x0001_0101,
        };
        fields.push(id);
        if id == 1 {
            fields.push(2);
            fields.extend([0, 1_440]);
            fields.extend([0; 14]);
            fields.extend([0x0000_0000, 0x00FF_FFFF]);
            fields.extend([0; 14]);
        } else {
            fields.push(1);
            fields.extend([0; 16]);
            fields.push(color);
            fields.extend([0; 15]);
        }
    }
    create_wdbc(18, 34, &fields, &[0])
}

/// Builds all six one-key scalar rows for LightParams ID one.
fn constant_light_float_bands() -> Vec<u8> {
    let values = [3_600.0_f32, 0.5, 0.25, 0.75, 1.25, 1.5];
    let mut fields = Vec::with_capacity(6 * 34);
    for (index, value) in values.into_iter().enumerate() {
        fields.push(index as u32 + 1);
        fields.push(1);
        fields.extend([0; 16]);
        fields.push(value.to_bits());
        fields.extend([0; 15]);
    }
    create_wdbc(6, 34, &fields, &[0])
}

/// Sound identities retain authored variation order and ordinary patch precedence.
#[test]
fn sound_entry_catalog_decodes_stock_paths_and_weights() -> Result<(), Box<dyn Error>> {
    let base_table = sound_entries_fixture(12, "Base", "Sound\\Base", "old.wav", "", 1, 0);
    let patch_table = sound_entries_fixture(
        77,
        "SwordImpact",
        "\\Sound\\Item\\Weapons",
        "impact-a.wav",
        "impact-b.mp3",
        25,
        75,
    );
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\SoundEntries.dbc",
            bytes: &base_table,
        },
        FixtureFile {
            archive: "patch-2.MPQ",
            path: "DBFilesClient\\SoundEntries.dbc",
            bytes: &patch_table,
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;

    let catalog = SoundEntryCatalog::load(&mut store)?;
    let entry = catalog.entry(77).ok_or("sound row was not indexed")?;
    assert_eq!(catalog.entries().len(), 1);
    assert_eq!(entry.id(), 77);
    assert_eq!(entry.sound_type(), 4);
    assert_eq!(entry.internal_name(), "SwordImpact");
    assert_eq!(entry.volume(), 0.75);
    assert_eq!(entry.flags(), 0x20);
    assert_eq!(entry.minimum_distance(), 8.0);
    assert_eq!(entry.distance_cutoff(), 50.0);
    assert_eq!(entry.eax_definition_id(), 12);
    assert_eq!(entry.advanced_id(), 90);
    assert_eq!(entry.assets().len(), 2);
    assert_eq!(
        entry.assets()[0].path().as_str(),
        "SOUND\\ITEM\\WEAPONS\\IMPACT-A.WAV"
    );
    assert_eq!(entry.assets()[0].frequency(), 25);
    assert_eq!(
        entry.assets()[1].path().as_str(),
        "SOUND\\ITEM\\WEAPONS\\IMPACT-B.MP3"
    );
    assert_eq!(entry.assets()[1].frequency(), 75);
    assert!(catalog.entry(12).is_none());
    Ok(())
}

/// Blizzard's shipped UNC build path remains an archive identity, not a host path.
#[test]
fn sound_entry_catalog_retains_stock_build_machine_path() -> Result<(), Box<dyn Error>> {
    let table = sound_entries_fixture(
        8912,
        "ID_Forge_Zap04",
        "\\\\guldan\\Drive2\\projects\\WoW\\FinalData\\Patch_3.0.1\\Data\\Sound\\D",
        "ID_Forge_Zap04",
        "",
        1,
        0,
    );
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\SoundEntries.dbc",
        bytes: &table,
    }])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;

    let catalog = SoundEntryCatalog::load(&mut store)?;
    let entry = catalog
        .entry(8912)
        .ok_or("stock sound row was not indexed")?;
    assert_eq!(entry.assets().len(), 1);
    assert_eq!(
        entry.assets()[0].path().as_str(),
        "GULDAN\\DRIVE2\\PROJECTS\\WOW\\FINALDATA\\PATCH_3.0.1\\DATA\\SOUND\\D\\ID_FORGE_ZAP04"
    );
    Ok(())
}

/// Another client-version layout is rejected rather than guessed.
#[test]
fn sound_entry_catalog_rejects_non_stock_layout() -> Result<(), Box<dyn Error>> {
    let table = create_wdbc(1, 29, &[0; 29], &[0]);
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\SoundEntries.dbc",
        bytes: &table,
    }])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;

    assert!(matches!(
        SoundEntryCatalog::load(&mut store),
        Err(AssetError::DatabaseDecode { path, .. })
            if path == AssetPath::new("DBFilesClient\\SoundEntries.dbc")?
    ));
    Ok(())
}

/// Invalid authored media paths do not receive a loose-file or basename fallback.
#[test]
fn sound_entry_catalog_rejects_invalid_asset_path() -> Result<(), Box<dyn Error>> {
    let table = sound_entries_fixture(77, "Invalid", "Sound\\..", "escape.wav", "", 1, 0);
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\SoundEntries.dbc",
        bytes: &table,
    }])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;

    assert!(matches!(
        SoundEntryCatalog::load(&mut store),
        Err(AssetError::DatabaseDecode { .. })
    ));
    Ok(())
}

/// Advanced sound policy preserves every build-12340 field and patch precedence.
#[test]
fn advanced_sound_entry_catalog_decodes_complete_stock_row() -> Result<(), Box<dyn Error>> {
    let base_table = advanced_sound_entry_fixture(12, 11, "BasePolicy");
    let patch_table = advanced_sound_entry_fixture(90, 77, "WindTunnel");
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\SoundEntriesAdvanced.dbc",
            bytes: &base_table,
        },
        FixtureFile {
            archive: "patch-2.MPQ",
            path: "DBFilesClient\\SoundEntriesAdvanced.dbc",
            bytes: &patch_table,
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;

    let catalog = AdvancedSoundEntryCatalog::load(&mut store)?;
    let entry = catalog
        .entry(90)
        .ok_or("advanced sound row was not indexed")?;
    assert_eq!(catalog.entries().len(), 1);
    assert_eq!(entry.id(), 90);
    assert_eq!(entry.sound_entry_id(), 77);
    assert_eq!(entry.inner_radius_2d(), 8.0);
    assert_eq!(entry.times(), [10, 20, 30, 40]);
    assert_eq!(entry.random_offset_range(), 5);
    assert_eq!(entry.usage(), 2);
    assert_eq!(entry.time_interval_minimum(), 1_000);
    assert_eq!(entry.time_interval_maximum(), 3_000);
    assert_eq!(entry.volume_slider_category(), 3);
    assert_eq!(entry.duck_to_sfx(), 0.25);
    assert_eq!(entry.duck_to_music(), 0.5);
    assert_eq!(entry.duck_to_ambience(), 0.75);
    assert_eq!(entry.inner_radius_of_influence(), 12.0);
    assert_eq!(entry.outer_radius_of_influence(), 48.0);
    assert_eq!(entry.time_to_duck(), 250);
    assert_eq!(entry.time_to_unduck(), 500);
    assert_eq!(entry.inside_angle(), 90.0);
    assert_eq!(entry.outside_angle(), 180.0);
    assert_eq!(entry.outside_volume(), 0.2);
    assert_eq!(entry.outer_radius_2d(), 64.0);
    assert_eq!(entry.name(), "WindTunnel");
    assert!(catalog.entry(12).is_none());
    Ok(())
}

/// Another client-version layout is not partially decoded as build 12340.
#[test]
fn advanced_sound_entry_catalog_rejects_non_stock_layout() -> Result<(), Box<dyn Error>> {
    let table = create_wdbc(1, 25, &[0; 25], &[0]);
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\SoundEntriesAdvanced.dbc",
        bytes: &table,
    }])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;

    assert!(matches!(
        AdvancedSoundEntryCatalog::load(&mut store),
        Err(AssetError::DatabaseDecode { path, .. })
            if path == AssetPath::new("DBFilesClient\\SoundEntriesAdvanced.dbc")?
    ));
    Ok(())
}

/// Invalid authored floating-point policy has no zero-value fallback.
#[test]
fn advanced_sound_entry_catalog_rejects_nonfinite_policy() -> Result<(), Box<dyn Error>> {
    let mut table = advanced_sound_entry_fixture(90, 77, "Invalid");
    let record_offset = 20 + 16 * 4;
    table[record_offset..record_offset + 4].copy_from_slice(&f32::NAN.to_bits().to_le_bytes());
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\SoundEntriesAdvanced.dbc",
        bytes: &table,
    }])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;

    assert!(matches!(
        AdvancedSoundEntryCatalog::load(&mut store),
        Err(AssetError::DatabaseDecode { .. })
    ));
    Ok(())
}

/// Builds one exact 30-field `SoundEntries.dbc` row.
fn sound_entries_fixture(
    id: u32,
    name: &str,
    directory: &str,
    first_file: &str,
    second_file: &str,
    first_frequency: u32,
    second_frequency: u32,
) -> Vec<u8> {
    let mut strings = vec![0];
    let name = append_string(&mut strings, name);
    let first_file = append_string(&mut strings, first_file);
    let second_file = append_string(&mut strings, second_file);
    let directory = append_string(&mut strings, directory);
    let mut fields = vec![id, 4, name, first_file, second_file];
    fields.extend([0; 8]);
    fields.extend([first_frequency, second_frequency]);
    fields.extend([0; 8]);
    fields.extend([
        directory,
        0.75_f32.to_bits(),
        0x20,
        8.0_f32.to_bits(),
        50.0_f32.to_bits(),
        12,
        90,
    ]);
    create_wdbc(1, 30, &fields, &strings)
}

/// Builds one complete 24-field `SoundEntriesAdvanced.dbc` row.
fn advanced_sound_entry_fixture(id: u32, sound_entry_id: u32, name: &str) -> Vec<u8> {
    let mut strings = vec![0];
    let name = append_string(&mut strings, name);
    let fields = [
        id,
        sound_entry_id,
        8.0_f32.to_bits(),
        10,
        20,
        30,
        40,
        5,
        2,
        1_000,
        3_000,
        3,
        0.25_f32.to_bits(),
        0.5_f32.to_bits(),
        0.75_f32.to_bits(),
        12.0_f32.to_bits(),
        48.0_f32.to_bits(),
        250,
        500,
        90.0_f32.to_bits(),
        180.0_f32.to_bits(),
        0.2_f32.to_bits(),
        64.0_f32.to_bits(),
        name,
    ];
    create_wdbc(1, 24, &fields, &strings)
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

/// Generates an explicitly packed WDBC layout such as `CharBaseInfo.dbc`.
fn create_packed_wdbc(
    record_count: u32,
    field_count: u32,
    record_size: u32,
    records: &[u8],
    string_block: &[u8],
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(20 + records.len() + string_block.len());
    bytes.extend_from_slice(b"WDBC");
    bytes.extend_from_slice(&record_count.to_le_bytes());
    bytes.extend_from_slice(&field_count.to_le_bytes());
    bytes.extend_from_slice(&record_size.to_le_bytes());
    bytes.extend_from_slice(&(string_block.len() as u32).to_le_bytes());
    bytes.extend_from_slice(records);
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
