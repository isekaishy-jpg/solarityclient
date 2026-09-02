//! Stock-layout tests for creature-family pet scale interpolation.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, CreatureFamilyCatalog, Locale};

use crate::support::{Fixture, FixtureFile};

/// Build 12340 clamps pet level before interpolating the family scale interval.
#[test]
fn creature_family_catalog_interpolates_stock_pet_scale() -> Result<(), Box<dyn Error>> {
    let mut first = [0_u32; 28];
    first[..5].copy_from_slice(&[17, 0.8_f32.to_bits(), 10, 1.2_f32.to_bits(), 30]);
    let mut second = [0_u32; 28];
    second[..5].copy_from_slice(&[23, 2.0_f32.to_bits(), 40, 3.0_f32.to_bits(), 40]);
    let fields = [first, second].concat();
    let table = create_wdbc(2, 28, &fields);
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\CreatureFamily.dbc",
        bytes: &table,
    }])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let catalog = CreatureFamilyCatalog::load(&mut store)?;

    let family = catalog.family(17).ok_or("family row was not indexed")?;
    assert_eq!(family.minimum_scale(), (0.8, 10));
    assert_eq!(family.maximum_scale(), (1.2, 30));
    assert_eq!(family.scale_for_level(1), Some(0.8));
    assert!((family.scale_for_level(20).ok_or("midpoint was absent")? - 1.0).abs() < f32::EPSILON);
    assert_eq!(family.scale_for_level(80), Some(1.2));
    assert_eq!(
        catalog.family(23).and_then(|row| row.scale_for_level(40)),
        None
    );
    assert_eq!(catalog.family(99), None);
    Ok(())
}

/// Serializes one fixed-width WDBC table without relying on production helpers.
fn create_wdbc(record_count: u32, field_count: u32, fields: &[u32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(21 + fields.len() * 4);
    bytes.extend_from_slice(b"WDBC");
    bytes.extend_from_slice(&record_count.to_le_bytes());
    bytes.extend_from_slice(&field_count.to_le_bytes());
    bytes.extend_from_slice(&(field_count * 4).to_le_bytes());
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    for field in fields {
        bytes.extend_from_slice(&field.to_le_bytes());
    }
    bytes.push(0);
    bytes
}
