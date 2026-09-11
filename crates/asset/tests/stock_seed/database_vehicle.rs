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
    first[3..6].copy_from_slice(&[
        1.25_f32.to_bits(),
        (-0.0_f32).to_bits(),
        (-3.5_f32).to_bits(),
    ]);
    first[6..13].copy_from_slice(&[1., 2., 3., 4., 5., 6., 7.].map(f32::to_bits));
    first[19..26].copy_from_slice(&[8., 9., 10., 11., 12., 13., 14.].map(f32::to_bits));
    first[13..15].copy_from_slice(&[u32::MAX, 37]);
    first[26..28].copy_from_slice(&[187, 39]);
    first[29..32].copy_from_slice(&[0.1, 0.2, -0.3].map(f32::to_bits));
    first[32] = u32::MAX;
    first[33..39].copy_from_slice(&[37, 39, 91, u32::MAX, 4, 34]);
    first[45] = 0x1234_5678;
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
    let seat = catalog.passenger_seat(1, 7).ok_or("seat")?;
    assert_eq!(seat.attachment_offset().map(f32::to_bits), first[3..6]);
    assert_eq!(seat.passenger_rotation().map(f32::to_bits), first[29..32]);
    assert_eq!(seat.enter_transition().map(f32::to_bits), first[6..13]);
    assert_eq!(seat.exit_transition().map(f32::to_bits), first[19..26]);
    assert_eq!(seat.enter_animations(), [-1, 37]);
    assert_eq!(seat.exit_animations(), [187, 39]);
    assert_eq!(seat.passenger_attachment_id(), -1);
    assert_eq!(seat.vehicle_animations(), [37, 39, 91]);
    assert_eq!(seat.vehicle_animation_keys(), [-1, 4, 34]);
    assert_eq!(seat.flags_b(), 0x1234_5678);
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
