//! External stock-compatibility tests for client-authored combat coefficients.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, CombatStatCatalog, Locale};

use crate::support::{Fixture, FixtureFile};

/// Melee critical-strike chance preserves the stock class-major table lookup.
#[test]
fn melee_critical_strike_catalog_uses_class_and_level_rows() -> Result<(), Box<dyn Error>> {
    let mut bases = vec![0.0_f32; 11];
    bases[0] = 0.05;
    let mut coefficients = vec![0.0_f32; 1_100];
    coefficients[0] = 0.01;
    coefficients[100 + 1] = 0.02;
    let base_table = float_wdbc(&bases);
    let coefficient_table = float_wdbc(&coefficients);
    let mut spell_bases = vec![0.0_f32; 11];
    spell_bases[0] = 0.03;
    let mut spell_coefficients = vec![0.0_f32; 1_100];
    spell_coefficients[0] = 0.005;
    let spell_base_table = float_wdbc(&spell_bases);
    let spell_coefficient_table = float_wdbc(&spell_coefficients);
    let mut oct_health_regen = vec![0.0_f32; 1_100];
    oct_health_regen[0] = 0.1;
    let mut health_regen_per_spirit = vec![0.0_f32; 1_100];
    health_regen_per_spirit[0] = 0.2;
    let oct_health_regen_table = float_wdbc(&oct_health_regen);
    let health_regen_per_spirit_table = float_wdbc(&health_regen_per_spirit);
    let mut mana_regen_per_spirit = vec![0.0_f32; 1_100];
    mana_regen_per_spirit[0] = 0.01;
    let mana_regen_per_spirit_table = float_wdbc(&mana_regen_per_spirit);
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\gtChanceToMeleeCritBase.dbc",
            bytes: &base_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\gtChanceToMeleeCrit.dbc",
            bytes: &coefficient_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\gtChanceToSpellCritBase.dbc",
            bytes: &spell_base_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\gtChanceToSpellCrit.dbc",
            bytes: &spell_coefficient_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\gtOCTRegenHP.dbc",
            bytes: &oct_health_regen_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\gtRegenHPPerSpt.dbc",
            bytes: &health_regen_per_spirit_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\gtRegenMPPerSpt.dbc",
            bytes: &mana_regen_per_spirit_table,
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;

    let catalog = CombatStatCatalog::load(&mut store)?;
    assert_near(catalog.chance_from_agility(1, 1, 20), 25.0)?;
    assert_near(catalog.chance_from_agility(2, 2, 10), 20.0)?;
    assert_near(catalog.chance_from_agility(1, 1, -20), 5.0)?;
    assert_near(catalog.spell_chance_from_intellect(1, 1, 20), 13.0)?;
    assert_near(catalog.health_regen_from_spirit(1, 1, 60), 7.0)?;
    assert_near(catalog.mana_regen_from_spirit(1, 1, 100, 20), 2.001)?;
    assert_eq!(catalog.chance_from_agility(0, 1, 20), None);
    assert_eq!(catalog.chance_from_agility(1, 0, 20), None);
    Ok(())
}

fn assert_near(actual: Option<f64>, expected: f64) -> Result<(), Box<dyn Error>> {
    let actual = actual.ok_or("critical-strike coefficient is absent")?;
    assert!(
        (actual - expected).abs() < 0.000_01,
        "{actual} != {expected}"
    );
    Ok(())
}

fn float_wdbc(values: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(20 + values.len() * 4 + 1);
    bytes.extend_from_slice(b"WDBC");
    bytes.extend_from_slice(&(values.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&4_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    for value in values {
        bytes.extend_from_slice(&value.to_bits().to_le_bytes());
    }
    bytes.push(0);
    bytes
}
