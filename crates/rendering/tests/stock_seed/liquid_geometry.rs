//! Full native liquid geometry comparisons through the decoded MH2O boundary.

use std::error::Error;

use solarity_asset::{
    ArchiveCatalog, AssetStore, ClientDataRoot, Locale, MapCatalog, TerrainMap, TerrainTileIndex,
};
use solarity_rendering::{LiquidDepthCoordinates, TerrainLiquidMeshPlan};
use wow_adt::{AdtVersion, builder::AdtBuilder};

use crate::{
    support::{Fixture, FixtureFile},
    terrain::{map_table, terrain_wdt},
};

/// Every position/UV bit and strip index comes from original 7CDF80/7CE390/7CE270.
#[test]
fn terrain_liquid_mesh_matches_native_vertices_and_masked_strips() -> Result<(), Box<dyn Error>> {
    let fixture_bytes = include_bytes!("../fixtures/liquid_terrain_geometry.bin");
    let mut remaining = fixture_bytes.as_slice();
    let mut records = Vec::new();
    let mut payload = vec![0_u8; 256 * 12];
    while !remaining.is_empty() {
        let count = word(remaining, 16) as usize;
        let index_count = word(remaining, 17) as usize;
        let length = 72 + count * 53 + index_count * 2;
        let record = &remaining[..length];
        append_layer(&mut payload, records.len(), record);
        records.push(record);
        remaining = &remaining[length..];
    }
    assert_eq!(records.len(), 36);
    let mut adt = AdtBuilder::new()
        .with_version(AdtVersion::WotLK)
        .add_texture("tileset/fixture/waterbed.blp")
        .build()?
        .to_bytes()?;
    adt.extend_from_slice(b"O2HM");
    adt.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    adt.extend_from_slice(&payload);
    let map_table = map_table();
    let wdt = terrain_wdt()?;
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "DBFilesClient\\Map.dbc",
            bytes: &map_table,
        },
        FixtureFile {
            path: "World\\Maps\\Northrend\\Northrend.wdt",
            bytes: &wdt,
        },
        FixtureFile {
            path: "World\\Maps\\Northrend\\Northrend_32_32.adt",
            bytes: &adt,
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let maps = MapCatalog::load(&mut store)?;
    let map = TerrainMap::load(&mut store, maps.map(571).ok_or("fixture map missing")?)?;
    let tile = map.load_tile(
        &mut store,
        TerrainTileIndex::new(32, 32).ok_or("invalid fixture tile")?,
    )?;
    let liquids = tile.liquids().ok_or("missing fixture MH2O")?;
    for (case, record) in records.iter().enumerate() {
        let layer = &liquids.chunks()[case].layers()[0];
        let depth = match word(record, 1) {
            0 => Some(LiquidDepthCoordinates::River),
            1 => Some(LiquidDepthCoordinates::Ocean),
            2 => None,
            _ => unreachable!(),
        };
        let mesh = TerrainLiquidMeshPlan::prepare(
            layer,
            f32::from_bits(word(record, 10)),
            std::array::from_fn(|axis| f32::from_bits(word(record, 11 + axis))),
            depth,
        );
        let count = word(record, 16) as usize;
        let expected = &record[72 + count * 9..];
        assert_eq!(mesh.vertices().len(), count);
        for (index, vertex) in mesh.vertices().iter().enumerate() {
            let original = &expected[index * 44..][..44];
            assert_eq!(
                vertex.position().map(f32::to_bits),
                std::array::from_fn(|axis| word(original, axis)),
                "position case {case} vertex {index}"
            );
            assert_eq!(
                [word(original, 3), word(original, 4), word(original, 5)],
                [0, 0, 1.0_f32.to_bits()]
            );
            assert_eq!(word(original, 6), u32::MAX);
            assert_eq!(
                vertex.depth_coordinates().map(f32::to_bits),
                [word(original, 7), word(original, 8)],
                "depth case {case} vertex {index}"
            );
            assert_eq!(
                vertex.surface_coordinates().map(f32::to_bits),
                [word(original, 9), word(original, 10)],
                "surface case {case} vertex {index}"
            );
        }
        let expected_indices: Vec<u16> = expected[count * 44..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|bytes| u16::from_le_bytes(*bytes))
            .collect();
        assert_eq!(mesh.indices(), expected_indices, "strip case {case}");
    }
    Ok(())
}

/// Encodes each oracle input into an independent MH2O chunk for real asset decoding.
fn append_layer(payload: &mut Vec<u8>, chunk: usize, record: &[u8]) {
    let count = word(record, 16) as usize;
    let format = word(record, 0);
    let offset = payload.len();
    payload[chunk * 12..][..4].copy_from_slice(&(offset as u32).to_le_bytes());
    payload[chunk * 12 + 4..][..4].copy_from_slice(&1_u32.to_le_bytes());
    payload.extend_from_slice(&1_u16.to_le_bytes());
    payload.extend_from_slice(&(format as u16).to_le_bytes());
    payload.extend_from_slice(&0.0_f32.to_le_bytes());
    payload.extend_from_slice(&100.0_f32.to_le_bytes());
    payload.extend((2..6).map(|index| word(record, index) as u8));
    payload.extend_from_slice(&((offset + 24) as u32).to_le_bytes());
    payload.extend_from_slice(&((offset + 32) as u32).to_le_bytes());
    payload.extend_from_slice(&record[56..64]);
    if format != 2 {
        payload.extend_from_slice(&record[72..72 + count * 4]);
    }
    if matches!(format, 1 | 3) {
        payload.extend_from_slice(&record[72 + count * 5..72 + count * 9]);
    }
    if format != 1 {
        payload.extend_from_slice(&record[72 + count * 4..72 + count * 5]);
    }
}

fn word(bytes: &[u8], index: usize) -> u32 {
    let offset = index * 4;
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}
