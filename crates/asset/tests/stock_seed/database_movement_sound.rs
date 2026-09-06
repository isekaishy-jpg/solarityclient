//! Armor foley follows the original Material table, including authored silence.

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale, MovementSoundCatalog};
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
