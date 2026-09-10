//! Exact ScreenEffect fields and condition rejection through mounted archives.

use crate::support::{Fixture, FixtureFile};
use solarity_asset::{
    ArchiveCatalog, AssetStore, ClientDataRoot, Locale, ScreenEffectCatalog, WorldLightCondition,
};
use std::error::Error;

fn table(rows: &[[u32; 10]]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    bytes.extend(
        [rows.len() as u32, 10, 40, 1]
            .into_iter()
            .flat_map(u32::to_le_bytes),
    );
    bytes.extend(rows.iter().flatten().flat_map(|word| word.to_le_bytes()));
    bytes.push(0);
    bytes
}

fn load(bytes: &[u8]) -> Result<ScreenEffectCatalog, Box<dyn Error>> {
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient/ScreenEffect.dbc",
        bytes,
    }])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    Ok(ScreenEffectCatalog::load(&mut store)?)
}

#[test]
fn screen_effect_preserves_raw_arguments_and_rejects_out_of_range_conditions()
-> Result<(), Box<dyn Error>> {
    let mut rows = Vec::new();
    for (index, condition) in [0, 1, 2, 3, 4, 5, 6, 7, 8, 255, 256, u32::MAX]
        .into_iter()
        .enumerate()
    {
        rows.push([
            index as u32 + 1,
            0,
            index as u32 % 4,
            0xff00aabb,
            3,
            40,
            u32::MAX,
            condition,
            462,
            36,
        ]);
    }
    let catalog = load(&table(&rows))?;
    for row in &rows {
        let effect = catalog.definition(row[0]).ok_or("effect")?;
        assert_eq!(effect.effect_type, row[2]);
        assert_eq!(effect.parameters, [0xff00aabb, 3, 40, u32::MAX]);
        assert_eq!(
            effect.light_condition,
            if row[7] < 8 {
                WorldLightCondition::new(row[7] as u8)
            } else {
                None
            }
        );
        assert_eq!(effect.sound_references, [462, 36]);
    }
    assert!(catalog.definition(0).is_none());
    assert!(load(&table(&[rows[0], rows[0]])).is_err());
    let mut wrong_layout = table(&rows);
    wrong_layout[8..12].copy_from_slice(&9_u32.to_le_bytes());
    assert!(load(&wrong_layout).is_err());
    Ok(())
}
