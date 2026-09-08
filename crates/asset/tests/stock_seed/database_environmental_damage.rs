//! Environmental category assignment follows physical rows, independently of IDs.

use crate::support::{Fixture, FixtureFile};
use solarity_asset::{
    ArchiveCatalog, AssetStore, ClientDataRoot, EnvironmentalDamageCatalog, Locale,
};
use std::error::Error;

fn table<const N: usize>(rows: &[[u32; N]]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for word in [rows.len() as u32, N as u32, N as u32 * 4, 1] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    for word in rows.iter().flatten() {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes.push(0);
    bytes
}

fn load(mapping: &[u8], kits: &[u8]) -> Result<EnvironmentalDamageCatalog, Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient/EnvironmentalDamage.dbc",
            bytes: mapping,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient/SpellVisualKit.dbc",
            bytes: kits,
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    Ok(EnvironmentalDamageCatalog::load(&mut store)?)
}

#[test]
fn environmental_damage_preserves_native_category_order_and_kit_parameters()
-> Result<(), Box<dyn Error>> {
    let mut kit = [0_u32; 38];
    kit[0] = 870;
    kit[1] = u32::MAX;
    kit[2] = 9;
    kit[4] = 632;
    kit[5] = 213;
    kit[8] = 754;
    kit[15] = 5736;
    kit[17..21].fill(u32::MAX);
    kit[18] = 13;
    for (index, value) in [0.2_f32, 0.4, 2.0, 3.0].into_iter().enumerate() {
        kit[22 + index * 4] = value.to_bits();
    }
    let mapping = table(&[
        [99, 1, 870],
        [1, 1, 999],
        [3, 6, 870],
        [4, u32::MAX, 870],
        [5, 2, 870],
        [6, 3, 999],
    ]);
    let catalog = load(&mapping, &table(&[kit]))?;
    let selected = catalog.visual_kit(1).ok_or("selected kit")?;
    assert_eq!(selected.words(), &kit);
    assert_eq!(selected.animation(), 9);
    assert_eq!(selected.sound_entry_id(), 5736);
    assert_eq!(
        selected.effects().collect::<Vec<_>>(),
        vec![(19, 213), (17, 754), (34, 632)]
    );
    assert_eq!(
        selected.special_effects().collect::<Vec<_>>(),
        vec![(13, [0.2, 0.4, 2.0, 3.0])]
    );
    assert_eq!(catalog.visual_kit(2), Some(selected));
    assert_eq!(catalog.visual_kits().count(), 1);
    for kind in [0, 3, 4, 5, 6, 255] {
        assert!(catalog.visual_kit(kind).is_none());
    }
    assert!(load(&mapping, &table(&[kit, kit])).is_err());
    assert!(load(&table(&[[1, 1]]), &table(&[kit])).is_err());
    assert!(load(&mapping, &table(&[[870; 37]])).is_err());
    Ok(())
}
