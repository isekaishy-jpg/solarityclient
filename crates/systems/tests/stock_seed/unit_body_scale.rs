//! Original 722AE0/71C110/71C050 results through real catalogs and ECS fields.

use std::error::Error;

use glam::Vec3;
use solarity_asset::{
    ArchiveCatalog, AssetStore, CharacterRaceCatalog, ClientDataRoot, CreatureCatalog,
    CreatureFamilyCatalog, Locale,
};
use solarity_ecs::{ActiveWorld, ObjectKind, WorldBootstrap, WorldMapId};
use solarity_systems::resolve_unit_body_scale;

use crate::support::{Fixture, FixtureFile};

/// Provider absence, humanoid race scale, signed family clamps and pet override
/// are captured by tools/ghidra/unit_body_scale_oracle.py from the pinned PE.
#[test]
fn unit_body_scale_matches_unhooked_native_providers() -> Result<(), Box<dyn Error>> {
    let bytes = include_bytes!("../fixtures/unit-body-scale-native.bin");
    assert_eq!(&bytes[..8], b"UBS12340");
    let word = |offset| u32::from_le_bytes(std::array::from_fn(|index| bytes[offset + index]));
    let count = word(8) as usize;
    assert_eq!(bytes.len(), 12 + count * 56);
    let mut displays = Vec::new();
    let mut models = Vec::new();
    let mut extras = Vec::new();
    let mut races = Vec::new();
    let mut families = Vec::new();
    let cases: Vec<[u32; 14]> = (0..count)
        .map(|index| std::array::from_fn(|column| word(12 + index * 56 + column * 4)))
        .collect();
    for (index, case) in cases.iter().enumerate() {
        let id = 100 + index as u32 * 3;
        let absent = case[12];
        if absent != 1 {
            let mut row = [0; 16];
            row[0] = id;
            row[1] = id;
            row[3] = if case[3] != 0 { id } else { 0 };
            row[4] = case[0];
            displays.extend(row);
        }
        if absent != 5 {
            for gender in 0..2 {
                let mut row = [0; 16];
                row[0] = id + 1 + gender;
                row[4] = case[2];
                displays.extend(row);
            }
        }
        if absent != 2 {
            let mut row = [0; 28];
            row[0] = id;
            row[4] = case[1];
            models.extend(row);
        }
        if absent != 3 {
            let mut row = [0; 21];
            row[0] = id;
            row[1] = id;
            row[2] = case[11];
            extras.extend(row);
        }
        if absent != 4 {
            let mut row = [0; 69];
            row[0] = id;
            row[4] = id + 1;
            row[5] = id + 2;
            races.extend(row);
        }
        if absent != 6 && case[4] != 0 {
            let mut row = [0; 28];
            row[0] = id;
            row[1..5].copy_from_slice(&case[5..9]);
            families.extend(row);
        }
    }
    let tables = [
        (
            "DBFilesClient\\CreatureDisplayInfo.dbc",
            table(16, &displays),
        ),
        ("DBFilesClient\\CreatureModelData.dbc", table(28, &models)),
        (
            "DBFilesClient\\CreatureDisplayInfoExtra.dbc",
            table(21, &extras),
        ),
        ("DBFilesClient\\ChrRaces.dbc", table(69, &races)),
        ("DBFilesClient\\CreatureFamily.dbc", table(28, &families)),
    ];
    let files = tables
        .each_ref()
        .map(|(path, bytes)| FixtureFile { path, bytes });
    let fixture = Fixture::new(&files)?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let creatures = CreatureCatalog::load(&mut store)?;
    let races = CharacterRaceCatalog::load(&mut store)?;
    let families = CreatureFamilyCatalog::load(&mut store)?;
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        1,
        "Local",
        Vec3::ZERO,
        0.0,
    ));
    for (index, case) in cases.iter().enumerate() {
        let id = 100 + index as u32 * 3;
        let fields = [(67, id), (54, case[9]), (75, case[10])];
        world.create_object(2, ObjectKind::Unit, None, fields)?;
        let actual = resolve_unit_body_scale(&world, 2, &creatures, &races, families.family(id))
            .ok_or("missing scale")?;
        assert_eq!(actual.to_bits(), case[13], "native case {index}: {case:?}");
        world.remove_object(2)?;
    }
    assert_eq!(
        resolve_unit_body_scale(&world, 2, &creatures, &races, None),
        None
    );
    Ok(())
}

/// Encodes the same provider words in archive-backed WDBC tables.
fn table(width: u32, words: &[u32]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for value in [words.len() as u32 / width, width, width * 4, 1] {
        bytes.extend(value.to_le_bytes());
    }
    for value in words {
        bytes.extend(value.to_le_bytes());
    }
    bytes.push(0);
    bytes
}
