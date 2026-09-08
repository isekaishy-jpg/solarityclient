//! Native climate selection through decoded AreaTable and WMOAreaTable joins.

use super::RuntimeCharacterMetadata;
use crate::application::terrain_coordinator::UnitWorldModelLocation;
use crate::test_support::ClientFixture;
use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale, WorldModelAreaKey};
use std::error::Error;

#[test]
fn cold_area_selection_matches_original_unit_registration() -> Result<(), Box<dyn Error>> {
    let cases = include_str!("../fixtures/unit_cold_area.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            line.split_whitespace()
                .map(str::parse::<u32>)
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut areas = Vec::new();
    let mut world_models = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let base = index as u32 * 4;
        for (id, parent, flags) in [(1, 3, case[5]), (2, 3, case[6]), (3, 4, case[7]), (4, 0, 1)] {
            if id == 3 && case[8] == 0 {
                continue;
            }
            let mut row = [0_u32; 36];
            row[0] = base + id;
            row[2] = if parent == 0 { 0 } else { base + parent };
            row[4] = flags;
            areas.push(row);
        }
        for (ordinal, group_id, present, area) in
            [(0, 10, case[1], case[3]), (1, u32::MAX, case[2], case[4])]
        {
            if present == 0 {
                continue;
            }
            let mut row = [0_u32; 28];
            row[0] = index as u32 * 2 + ordinal + 1;
            row[1] = index as u32 + 1;
            row[3] = group_id;
            row[10] = if area == 0 {
                0
            } else if area == 999 {
                u32::MAX
            } else {
                base + area
            };
            world_models.push(row);
        }
    }
    let area_bytes = table(&areas);
    let world_model_bytes = table(&world_models);
    let fixture = ClientFixture::with_common_files(&[
        ("DBFilesClient\\AreaTable.dbc", &area_bytes),
        ("DBFilesClient\\WMOAreaTable.dbc", &world_model_bytes),
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let metadata = RuntimeCharacterMetadata::load(&mut store)?;
    for (index, case) in cases.iter().enumerate() {
        let location = (case[0] != 0).then_some(UnitWorldModelLocation {
            key: WorldModelAreaKey {
                root_id: index as u32 + 1,
                name_set: 0,
                group_id: 10,
            },
            world_model_only: true,
            area_override: true,
        });
        assert_eq!(
            metadata.cold_area_at(Some(index as u32 * 4 + 1), location),
            case[9] != 0,
            "case {index}: {case:?}"
        );
    }
    assert_eq!(cases.len(), 480);
    Ok(())
}

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
