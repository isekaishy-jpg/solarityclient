//! External stock-compatibility tests for WDBC bootstrap.

use std::error::Error;
use std::path::Path;

use solarity_asset::{
    ArchiveCatalog, AssetError, AssetPath, AssetStore, ClientDataRoot, CreatureCatalog, Locale,
    WdbcTable,
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
    let skin_1 = append_string(&mut display_strings, "BearSkinBrown");
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
        ["BearSkinBrown", "BearSkinBlack", ""]
    );
    assert_eq!(display.portrait_texture_name(), "BearPortrait");
    assert_eq!(display.size_class(), 2);
    assert_eq!(display.blood_id(), 3);
    assert_eq!(display.npc_sound_id(), 4);
    assert_eq!(display.particle_color_id(), 5);
    assert_eq!(display.geoset_data(), 0x0012_0304);
    assert_eq!(display.object_effect_package_id(), 6);

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
    Ok(())
}

/// Another client's layout is rejected rather than decoded through guessed offsets.
#[test]
fn creature_catalog_rejects_non_stock_layout() -> Result<(), Box<dyn Error>> {
    let display_table = create_wdbc(1, 15, &[0; 15], b"\0");
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
