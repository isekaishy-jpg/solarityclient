//! Complete WMO liquid geometry comparisons through real archive decoding.

use std::error::Error;

use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, DecodedWorldModel, Locale,
};
use solarity_rendering::{
    LiquidDepthCoordinates, WorldModelLiquidDepthColumn, WorldModelLiquidMeshPlan,
    WorldModelLiquidSurface,
};

use crate::{
    support::{Fixture, FixtureFile},
    world_model::{group_fixture, root_fixture},
};

/// Original 7A7CC0/7A7920/7A7F60 supplies every expected vertex bit and index.
#[test]
fn world_model_liquid_matches_native_grid_portal_clipping_and_uv_interpolation()
-> Result<(), Box<dyn Error>> {
    let mut remaining = include_bytes!("../fixtures/liquid_wmo_geometry.bin").as_slice();
    let mut records = Vec::new();
    let mut files = Vec::new();
    while !remaining.is_empty() {
        let length = 52
            + word(remaining, 9) as usize * 8
            + (word(remaining, 0) * word(remaining, 1)) as usize
            + word(remaining, 12) as usize * 36
            + word(remaining, 10) as usize * 44
            + word(remaining, 11) as usize * 2;
        let record = &remaining[..length];
        append_files(&mut files, records.len(), record);
        records.push(record);
        remaining = &remaining[length..];
    }
    assert_eq!(records.len(), 148);
    let fixture = Fixture::new(
        &files
            .iter()
            .map(|(path, bytes)| FixtureFile { path, bytes })
            .collect::<Vec<_>>(),
    )?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    for (case, record) in records.into_iter().enumerate() {
        let model = DecodedWorldModel::load(
            &mut store,
            &AssetPath::new(format!("World\\Wmo\\Liquid{case}.wmo"))?,
        )?;
        let argb = word(record, 5);
        let color = [
            (argb >> 16) as u8,
            (argb >> 8) as u8,
            argb as u8,
            (argb >> 24) as u8,
        ];
        let surface = if word(record, 2) == 0 {
            WorldModelLiquidSurface::Water {
                color,
                depth: match word(record, 3) {
                    0 => Some(LiquidDepthCoordinates::River),
                    1 => Some(LiquidDepthCoordinates::Ocean),
                    _ => None,
                },
                depth_column: if word(record, 4) == 0 {
                    WorldModelLiquidDepthColumn::Exterior
                } else {
                    WorldModelLiquidDepthColumn::Interior
                },
            }
        } else {
            WorldModelLiquidSurface::Magma { color }
        };
        let plan = WorldModelLiquidMeshPlan::prepare(&model, 0, surface)?
            .ok_or("missing fixture liquid")?;
        let expected = &record[52
            + word(record, 9) as usize * 8
            + (word(record, 0) * word(record, 1)) as usize
            + word(record, 12) as usize * 36..];
        assert_eq!(
            plan.vertices().len(),
            word(record, 10) as usize,
            "vertex count case {case}"
        );
        for (index, vertex) in plan.vertices().iter().enumerate() {
            let actual: Vec<u32> = vertex
                .to_bytes()
                .as_chunks::<4>()
                .0
                .iter()
                .map(|bytes| u32::from_le_bytes(*bytes))
                .collect();
            let expected: Vec<u32> = expected[index * 44..][..44]
                .as_chunks::<4>()
                .0
                .iter()
                .map(|bytes| u32::from_le_bytes(*bytes))
                .collect();
            assert_eq!(actual, expected, "vertex case {case} index {index}");
        }
        let expected: Vec<u16> = expected[plan.vertices().len() * 44..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|bytes| u16::from_le_bytes(*bytes))
            .collect();
        assert_eq!(plan.indices(), expected, "indices case {case}");
    }
    Ok(())
}

/// Encode oracle inputs as root/group chunks instead of constructing render data.
fn append_files(files: &mut Vec<(String, Vec<u8>)>, case: usize, record: &[u8]) {
    let width = word(record, 0);
    let height = word(record, 1);
    let input_count = word(record, 9) as usize;
    let plane_count = word(record, 12) as usize;
    let planes = &record[52 + input_count * 8 + (width * height) as usize..][..plane_count * 36];
    let mut root = root_fixture();
    chunk_mut(&mut root, *b"DHOM")[4..8].copy_from_slice(&((plane_count + 1) as u32).to_le_bytes());
    chunk_mut(&mut root, *b"DHOM")[8..12].copy_from_slice(&(plane_count as u32).to_le_bytes());
    // Replace the final MOGI table with all fully resident adjacent groups.
    let info = chunk_mut(&mut root, *b"IGOM").to_vec();
    root.truncate(root.len() - 40);
    push_chunk(&mut root, *b"IGOM", &info.repeat(plane_count + 1));
    let mut portal_bytes = Vec::new();
    let mut references = Vec::new();
    for (index, plane) in planes.as_chunks::<36>().0.iter().enumerate() {
        portal_bytes.extend(0u16.to_le_bytes());
        portal_bytes.extend(4u16.to_le_bytes());
        portal_bytes.extend(&plane[..16]);
        references.extend((index as u16).to_le_bytes());
        references.extend((index as u16 + 1).to_le_bytes());
        references.extend((word(plane, 4) as i16).to_le_bytes());
        references.extend(0u16.to_le_bytes());
        let neighbor_width = word(plane, 7);
        let neighbor_height = word(plane, 8);
        let mut grid = Vec::new();
        for value in [
            neighbor_width + 1,
            neighbor_height + 1,
            neighbor_width,
            neighbor_height,
            word(plane, 5),
            word(plane, 6),
            0,
        ] {
            grid.extend(value.to_le_bytes());
        }
        grid.extend(0u16.to_le_bytes());
        grid.resize(
            30 + ((neighbor_width + 1) * (neighbor_height + 1)) as usize * 8
                + (neighbor_width * neighbor_height) as usize,
            0,
        );
        files.push((
            format!("World\\Wmo\\Liquid{case}_{:03}.wmo", index + 1),
            liquid_group(&grid, 0),
        ));
    }
    push_chunk(&mut root, *b"VPOM", &[0; 48]);
    push_chunk(&mut root, *b"TPOM", &portal_bytes);
    push_chunk(&mut root, *b"RPOM", &references);
    files.push((format!("World\\Wmo\\Liquid{case}.wmo"), root));
    let mut grid = Vec::new();
    for value in [
        width + 1,
        height + 1,
        width,
        height,
        word(record, 6),
        word(record, 7),
        word(record, 8),
    ] {
        grid.extend(value.to_le_bytes());
    }
    grid.extend(0u16.to_le_bytes());
    grid.extend(&record[52..][..input_count * 8 + (width * height) as usize]);
    files.push((
        format!("World\\Wmo\\Liquid{case}_000.wmo"),
        liquid_group(&grid, plane_count as u16),
    ));
}

/// Attach a complete MLIQ while maintaining the enclosing MOGP extent.
fn liquid_group(grid: &[u8], portal_count: u16) -> Vec<u8> {
    let mut group = group_fixture();
    let header = chunk_mut(&mut group, *b"PGOM");
    header[8..12].copy_from_slice(&0x1000u32.to_le_bytes());
    header[38..40].copy_from_slice(&portal_count.to_le_bytes());
    let old_size = word(&group, 4);
    push_chunk(&mut group, *b"QILM", grid);
    group[16..20].copy_from_slice(&(old_size + grid.len() as u32 + 8).to_le_bytes());
    group
}

/// Locate fixture chunks without relying on their byte offsets.
fn chunk_mut(bytes: &mut [u8], magic: [u8; 4]) -> &mut [u8] {
    let mut offset = 0;
    while offset < bytes.len() {
        let length = word(&bytes[offset..], 1) as usize;
        if bytes[offset..offset + 4] == magic {
            return &mut bytes[offset + 8..offset + 8 + length];
        }
        offset += length + 8;
    }
    panic!("fixture chunk missing")
}

/// Write exact little-endian chunk framing.
fn push_chunk(bytes: &mut Vec<u8>, magic: [u8; 4], payload: &[u8]) {
    bytes.extend(magic);
    bytes.extend((payload.len() as u32).to_le_bytes());
    bytes.extend(payload);
}

/// Read one exact oracle word, including floating-point input bits.
fn word(bytes: &[u8], index: usize) -> u32 {
    let offset = index * 4;
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}
