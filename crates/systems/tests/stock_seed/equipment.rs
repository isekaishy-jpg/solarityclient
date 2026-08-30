//! External stock-compatibility tests for player equipment resolution.

use std::error::Error;

use glam::Vec3;
use solarity_asset::{
    ArchiveCatalog, AssetStore, ClientDataRoot, ItemDefinitionCatalog, ItemDisplayCatalog, Locale,
};
use solarity_ecs::{ActiveWorld, ObjectKind, PlayerEquipmentSlot, WorldBootstrap, WorldMapId};
use solarity_systems::{
    PlayerEquipmentAppearanceError, project_object_fields, resolve_player_equipment,
};

use crate::support::{Fixture, FixtureFile};

const PLAYER_VISIBLE_ITEM_1_ENTRY_ID: u16 = 283;
const PLAYER_VISIBLE_ITEM_1_ENCHANTMENT: u16 = 284;

/// Public item entry fields join through both exact build-12340 item tables.
#[test]
fn player_equipment_resolves_item_and_display_rows() -> Result<(), Box<dyn Error>> {
    let tables = item_tables();
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "DBFilesClient\\Item.dbc",
            bytes: &tables.items,
        },
        FixtureFile {
            path: "DBFilesClient\\ItemDisplayInfo.dbc",
            bytes: &tables.displays,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let definitions = ItemDefinitionCatalog::load(&mut store)?;
    let displays = ItemDisplayCatalog::load(&mut store)?;

    let guid = 0x42;
    let fields = [
        (PLAYER_VISIBLE_ITEM_1_ENTRY_ID, 50_001),
        (PLAYER_VISIBLE_ITEM_1_ENCHANTMENT, 0xCAFE_BABE),
    ];
    let mut world = player_world(guid);
    world.create_object(guid, ObjectKind::Player, None, fields)?;
    project_object_fields(&mut world, guid, fields)?;

    let equipment = resolve_player_equipment(&world, guid, &definitions, &displays)?;

    assert_eq!(equipment.guid(), guid);
    assert_eq!(equipment.items().len(), 1);
    let head = &equipment.items()[0];
    assert_eq!(head.slot(), PlayerEquipmentSlot::Head);
    assert_eq!(head.visible().entry_id(), 50_001);
    assert_eq!(head.visible().enchantment_word(), 0xCAFE_BABE);
    assert_eq!(head.definition().id(), 50_001);
    assert_eq!(head.definition().display_info_id(), 55_000);
    assert_eq!(head.display().id(), 55_000);
    assert_eq!(head.display().component_textures()[0], "Plate_ArmUpper");
    Ok(())
}

/// A nonzero unknown item remains an error instead of selecting another display.
#[test]
fn player_equipment_does_not_substitute_unknown_items() -> Result<(), Box<dyn Error>> {
    let tables = item_tables();
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "DBFilesClient\\Item.dbc",
            bytes: &tables.items,
        },
        FixtureFile {
            path: "DBFilesClient\\ItemDisplayInfo.dbc",
            bytes: &tables.displays,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let definitions = ItemDefinitionCatalog::load(&mut store)?;
    let displays = ItemDisplayCatalog::load(&mut store)?;

    let guid = 0x43;
    let fields = [(PLAYER_VISIBLE_ITEM_1_ENTRY_ID, 99_999)];
    let mut world = player_world(guid);
    world.create_object(guid, ObjectKind::Player, None, fields)?;
    project_object_fields(&mut world, guid, fields)?;

    assert_eq!(
        resolve_player_equipment(&world, guid, &definitions, &displays).err(),
        Some(PlayerEquipmentAppearanceError::MissingItem {
            slot: PlayerEquipmentSlot::Head,
            entry_id: 99_999,
        })
    );
    Ok(())
}

/// Two exact client tables needed for visible-item resolution.
struct ItemTables {
    items: Vec<u8>,
    displays: Vec<u8>,
}

/// Creates one item definition and its one display record.
fn item_tables() -> ItemTables {
    let items = create_wdbc(1, 8, &[50_001, 4, 1, u32::MAX, 1, 55_000, 1, 0], b"\0");

    let mut strings = vec![0];
    let model = append_string(&mut strings, "Helm_Plate_D_01.mdx");
    let model_texture = append_string(&mut strings, "Helm_Plate_D_01Red");
    let icon = append_string(&mut strings, "INV_Helmet_01");
    let arm_upper = append_string(&mut strings, "Plate_ArmUpper");
    let display_fields = [
        55_000,
        model,
        0,
        model_texture,
        0,
        icon,
        0,
        1,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        arm_upper,
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
    let displays = create_wdbc(1, 25, &display_fields, &strings);
    ItemTables { items, displays }
}

/// Creates an active world whose local object can receive a player create update.
fn player_world(guid: u64) -> ActiveWorld {
    ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        guid,
        "Solarion",
        Vec3::ZERO,
        0.0,
    ))
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
