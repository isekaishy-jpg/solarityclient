//! Exact transport schemas and native contiguous owner lookup.

use std::error::Error;

use solarity_asset::{
    ArchiveCatalog, AssetError, AssetStore, ClientDataRoot, Locale, TransportCatalog,
};

use crate::support::{Fixture, FixtureFile};

const PATHS: [&str; 4] = [
    "DBFilesClient\\TaxiPathNode.dbc",
    "DBFilesClient\\TransportPhysics.dbc",
    "DBFilesClient\\TransportAnimation.dbc",
    "DBFilesClient\\TransportRotation.dbc",
];

#[test]
fn transport_catalog_retains_all_words_and_stored_control_order() -> Result<(), Box<dyn Error>> {
    let taxi = vec![
        vec![1, u32::MAX, 0, 1, 0, 0, 0, 0, 0, 0, 0],
        vec![
            2,
            42,
            9,
            0,
            1.0_f32.to_bits(),
            (-2.0_f32).to_bits(),
            3.0_f32.to_bits(),
            2,
            7,
            101,
            102,
        ],
        vec![3, 42, 2, 1, 0, 0, 0, 1, 0, 0, 0],
        vec![4, 99, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    ];
    let physics = vec![
        (0..11)
            .map(|v| {
                if v == 0 {
                    21
                } else {
                    (v as f32 * 0.25).to_bits()
                }
            })
            .collect(),
    ];
    let animations = vec![vec![
        7,
        42,
        300,
        1.0_f32.to_bits(),
        2.0_f32.to_bits(),
        3.0_f32.to_bits(),
        5,
    ]];
    let rotations = vec![vec![
        8,
        42,
        500,
        0,
        0,
        (-0.5_f32).to_bits(),
        0.75_f32.to_bits(),
    ]];
    let tables = [
        table(11, &taxi),
        table(11, &physics),
        table(7, &animations),
        table(7, &rotations),
    ];
    let catalog = load(&tables)?;
    let path = catalog.path(42);
    assert_eq!(
        path.iter().map(|r| r.node_index).collect::<Vec<_>>(),
        [9, 2]
    );
    assert_eq!(path[0].id, 2);
    assert_eq!(path[0].position, [1.0, -2.0, 3.0]);
    assert_eq!(
        (
            path[0].flags,
            path[0].delay_seconds,
            path[0].arrival_event,
            path[0].departure_event
        ),
        (2, 7, 101, 102)
    );
    assert_eq!((path[1].map_id, path[1].flags), (1, 1));
    assert_eq!(catalog.path(u32::MAX).len(), 1);
    assert!(catalog.path(0).is_empty());
    assert!(catalog.path(43).is_empty());
    assert!(catalog.path(100).is_empty());
    assert_eq!(
        catalog.physics(21).ok_or("missing physics")?.parameters,
        std::array::from_fn(|i| (i + 1) as f32 * 0.25)
    );
    assert!(catalog.physics(0).is_none());
    let animation = catalog.animation(42)[0];
    assert_eq!(
        (animation.id, animation.time_ms, animation.sequence_id),
        (7, 300, 5)
    );
    assert_eq!(animation.position, [1.0, 2.0, 3.0]);
    let rotation = catalog.rotation(42)[0];
    assert_eq!((rotation.id, rotation.time_ms), (8, 500));
    assert_eq!(rotation.rotation, [0.0, 0.0, -0.5, 0.75]);
    assert!(catalog.animation(0).is_empty());
    assert!(catalog.rotation(43).is_empty());
    Ok(())
}

#[test]
fn transport_catalog_rejects_incompatible_layout_and_descending_owner_keys()
-> Result<(), Box<dyn Error>> {
    for index in 0..4 {
        let mut tables = [table(11, &[]), table(11, &[]), table(7, &[]), table(7, &[])];
        tables[index] = table(8, &[]);
        let error = load(&tables).err().ok_or("accepted wrong schema")?;
        assert!(
            error
                .downcast_ref::<AssetError>()
                .is_some_and(|e| matches!(e, AssetError::DatabaseDecode { .. }))
        );
    }
    let mut row = vec![0; 11];
    row[1] = 42;
    let mut previous = row.clone();
    previous[1] = 43;
    let tables = [
        table(11, &[previous, row]),
        table(11, &[]),
        table(7, &[]),
        table(7, &[]),
    ];
    assert!(load(&tables).is_err());
    Ok(())
}

/// Every test goes through archive resolution and the public catalog boundary.
fn load(tables: &[Vec<u8>; 4]) -> Result<TransportCatalog, Box<dyn Error>> {
    let files = std::array::from_fn::<_, 4, _>(|i| FixtureFile {
        archive: "common.MPQ",
        path: PATHS[i],
        bytes: &tables[i],
    });
    let fixture = Fixture::new(&files)?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    Ok(TransportCatalog::load(&mut store)?)
}

/// Encodes only synthetic records in the native fixed-width WDBC envelope.
fn table(fields: u32, rows: &[Vec<u32>]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for value in [rows.len() as u32, fields, fields * 4, 1] {
        bytes.extend(value.to_le_bytes());
    }
    for row in rows {
        for value in row {
            bytes.extend(value.to_le_bytes());
        }
    }
    bytes.push(0);
    bytes
}
