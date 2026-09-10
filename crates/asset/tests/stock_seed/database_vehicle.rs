//! Exact schemas, sparse references and independent flags/attachment words.

use crate::support::{Fixture, FixtureFile};
use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale, VehicleCatalog};
use std::error::Error;

#[test]
fn vehicle_catalog_preserves_seat_slots_and_signed_attachment_ids() -> Result<(), Box<dyn Error>> {
    let mut vehicle = [0_u32; 40];
    vehicle[0] = 1;
    vehicle[6..14].copy_from_slice(&[10, 11, 12, 0, 9, 13, 10, 12]);
    let mut first = [0_u32; 58];
    first[..3].copy_from_slice(&[12, 0x8000_0000, 21]);
    let mut second = [0_u32; 58];
    second[..3].copy_from_slice(&[10, 0, u32::MAX]);
    let fixture = fixture(&dbc(40, &vehicle), &dbc(58, &[first, second].concat()))?;
    let catalog = load(&fixture)?;
    assert_eq!(
        catalog
            .passenger_seat(1, 0)
            .map(|row| (row.flags(), row.attachment_id())),
        Some((0, -1))
    );
    assert_eq!(
        catalog
            .passenger_seat(1, 7)
            .map(|row| (row.flags(), row.attachment_id())),
        Some((0x8000_0000, 21))
    );
    for slot in [1, 3, 4, 5, 8, 127, -128, -1] {
        assert_eq!(catalog.passenger_seat(1, slot), None);
    }
    assert_eq!(catalog.vehicle(1).and_then(|row| row.seat_id(3)), Some(0));
    assert_eq!(catalog.passenger_seat(0, 0), None);
    assert_eq!(catalog.passenger_seat(2, 0), None);
    Ok(())
}

#[test]
fn vehicle_catalog_rejects_foreign_schemas_and_duplicate_keys() -> Result<(), Box<dyn Error>> {
    for (vehicle, seats) in [
        (dbc(39, &[]), dbc(58, &[])),
        (dbc(40, &[]), dbc(57, &[])),
        (dbc(40, &[0; 80]), dbc(58, &[])),
        (dbc(40, &[]), dbc(58, &[0; 116])),
    ] {
        assert!(load(&fixture(&vehicle, &seats)?).is_err());
    }
    Ok(())
}

fn load(fixture: &Fixture) -> Result<VehicleCatalog, Box<dyn Error>> {
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    Ok(VehicleCatalog::load(&mut AssetStore::mount(catalog)?)?)
}

fn fixture(vehicles: &[u8], seats: &[u8]) -> Result<Fixture, Box<dyn Error>> {
    Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient/Vehicle.dbc",
            bytes: vehicles,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient/VehicleSeat.dbc",
            bytes: seats,
        },
    ])
}

fn dbc(width: usize, fields: &[u32]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for value in [
        (fields.len() / width) as u32,
        width as u32,
        width as u32 * 4,
        1,
    ] {
        bytes.extend(value.to_le_bytes());
    }
    for value in fields {
        bytes.extend(value.to_le_bytes());
    }
    bytes.push(0);
    bytes
}
