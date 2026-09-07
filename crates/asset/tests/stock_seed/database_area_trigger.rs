//! Exact AreaTrigger layout and retained scan order.

use std::error::Error;

use solarity_asset::{
    ArchiveCatalog, AreaTriggerCatalog, AreaTriggerShape, AssetStore, ClientDataRoot, Locale,
};

use crate::support::{Fixture, FixtureFile};

#[test]
fn area_trigger_catalog_preserves_authored_shapes_and_order() -> Result<(), Box<dyn Error>> {
    let rows = [
        [
            80,
            1,
            1_f32.to_bits(),
            2_f32.to_bits(),
            3_f32.to_bits(),
            4_f32.to_bits(),
            99_f32.to_bits(),
            99_f32.to_bits(),
            99_f32.to_bits(),
            0,
        ],
        [
            19,
            1,
            0,
            0,
            0,
            0,
            8_f32.to_bits(),
            2_f32.to_bits(),
            6_f32.to_bits(),
            0.37_f32.to_bits(),
        ],
        [4, 2, 0, 0, 0, 1_f32.to_bits(), 0, 0, 0, 0],
    ];
    let mut bytes = b"WDBC".to_vec();
    for value in [3_u32, 10, 40, 1]
        .into_iter()
        .chain(rows.into_iter().flatten())
    {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.push(0);
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\AreaTrigger.dbc",
        bytes: &bytes,
    }])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let catalog = AreaTriggerCatalog::load(&mut store)?;
    assert_eq!(
        catalog
            .entries()
            .iter()
            .map(|entry| entry.id())
            .collect::<Vec<_>>(),
        [80, 19, 4]
    );
    assert_eq!(catalog.entries()[0].position(), [1., 2., 3.]);
    assert_eq!(
        catalog.entries()[0].shape(),
        AreaTriggerShape::Sphere { radius: 4. }
    );
    assert_eq!(
        catalog.entries()[1].shape(),
        AreaTriggerShape::Box {
            dimensions: [8., 2., 6.],
            rotation_radians: 0.37
        }
    );
    assert_eq!(catalog.entries()[2].map_id(), 2);
    Ok(())
}
