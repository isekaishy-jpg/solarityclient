//! Armor foley follows the original Material table, including authored silence.

use solarity_asset::{
    ArchiveCatalog, AreaTableCatalog, AssetStore, ClientDataRoot, LiquidTypeCatalog, Locale,
    MovementSoundCatalog,
};
use std::error::Error;

use crate::support::{Fixture, FixtureFile};

/// Native 4CFC10 reads AD41A8's row +8. That table's vtable at A283C0
/// calls loader 6489C0, whose filename getter 8B47D0 returns Material.dbc.
/// ItemGroupSounds has the same five-word layout but unrelated inventory cues.
#[test]
fn armor_foley_uses_material_table_and_preserves_silent_materials() -> Result<(), Box<dyn Error>> {
    let tables = [
        ("CreatureSoundData", 38, vec![]),
        ("TerrainType", 6, vec![]),
        ("FootstepTerrainLookup", 5, vec![]),
        ("GroundEffectTexture", 11, vec![]),
        ("LiquidType", 45, vec![]),
        ("AreaTable", 36, vec![]),
        // Build-12340 metal, chain, plate, and leather rows.
        (
            "Material",
            5,
            vec![
                1, 1, 0, 698, 700, 5, 5, 1005, 0, 0, 6, 3, 1004, 696, 701, 8, 0, 1003, 0, 0,
            ],
        ),
        // A colliding inventory key must never turn silent material 1 into
        // inventory sound 274.
        ("ItemGroupSounds", 5, vec![1, 273, 274, 275, 0]),
    ];
    let files = tables
        .into_iter()
        .map(|(name, fields, values)| (format!("DBFilesClient\\{name}.dbc"), dbc(fields, &values)))
        .collect::<Vec<_>>();
    let fixture = Fixture::new(
        &files
            .iter()
            .map(|(path, bytes)| FixtureFile {
                archive: "common.MPQ",
                path,
                bytes,
            })
            .collect::<Vec<_>>(),
    )?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let catalog = MovementSoundCatalog::load(&mut store)?;
    assert_eq!(catalog.armor(1), 0);
    assert_eq!(catalog.armor(5), 1005);
    assert_eq!(catalog.armor(6), 1004);
    assert_eq!(catalog.armor(8), 1003);
    assert_eq!(catalog.armor(u32::MAX), 0);
    Ok(())
}

/// Shared ripple/sound admission preserves 9905C0's low-ID slots and exactly
/// one parent lookup; missing rows and zero substitutions retain the input.
#[test]
fn area_liquid_flags_preserve_native_substitution_boundaries() -> Result<(), Box<dyn Error>> {
    let mut areas = Vec::new();
    for (id, parent, overrides) in [
        (100, 0, [30, 31, 0, 0]),
        (101, 100, [32, 0, 0, 0]),
        (102, 101, [0, 0, 0, 0]),
        (103, 999, [0, 0, 0, 0]),
        (104, 100, [999, 0, 0, 0]),
    ] {
        let mut row = [0; 36];
        row[0] = id;
        row[2] = parent;
        row[29..33].copy_from_slice(&overrides);
        areas.extend(row);
    }
    let mut liquids = Vec::new();
    for (id, flags) in [
        (1, 1),
        (2, 2),
        (5, 4),
        (20, 8),
        (21, 16),
        (30, 32),
        (31, 64),
        (32, 128),
    ] {
        let mut row = [0; 45];
        row[0] = id;
        row[2] = flags;
        liquids.extend(row);
    }
    let area_bytes = dbc(36, &areas);
    let liquid_bytes = dbc(45, &liquids);
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\AreaTable.dbc",
            bytes: &area_bytes,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\LiquidType.dbc",
            bytes: &liquid_bytes,
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let areas = AreaTableCatalog::load(&mut store)?;
    let liquids = LiquidTypeCatalog::load(&mut store)?;
    for (area, liquid, expected) in [
        (101, 0, None),
        (101, 1, Some(128)),
        (101, 2, Some(64)),
        (101, 5, Some(128)),
        (101, 20, Some(8)),
        (101, 21, Some(16)),
        (102, 1, Some(128)),
        (102, 2, Some(2)),
        (103, 1, Some(1)),
        (999, 1, Some(1)),
        (104, 1, None),
        (101, 999, None),
    ] {
        assert_eq!(
            areas.liquid_flags(&liquids, area, liquid),
            expected,
            "area {area} liquid {liquid}"
        );
    }
    Ok(())
}

/// Serializes the exact word layout with one valid empty DBC string.
fn dbc(fields: u32, values: &[u32]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for value in [values.len() as u32 / fields, fields, fields * 4, 1] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.push(0);
    bytes
}
