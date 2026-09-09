//! Complete native 7CC310 loading, 7D5150 upload and 7D5240 face-bank results.

use std::io::{Cursor, Read};

use solarity_asset::{AssetPath, TerrainLowDetail};
use solarity_rendering::TerrainLowDetailMesh;

#[test]
fn low_detail_tiles_match_native_loading_and_complete_mesh_uploads()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = include_bytes!("../fixtures/terrain-low-detail-native.bin");
    let mut input = Cursor::new(fixture.as_slice());
    assert_eq!(&read::<8>(&mut input)?, b"WDL12340");
    let count = u32::from_le_bytes(read(&mut input)?);
    let path = AssetPath::new("World/Maps/Native/Native.wdl")?;
    for _ in 0..count {
        let x = u32::from_le_bytes(read(&mut input)?);
        let y = u32::from_le_bytes(read(&mut input)?);
        let _base = read::<8>(&mut input)?;
        let bounds = read::<24>(&mut input)?;
        let heights = read::<1090>(&mut input)?;
        let masks = read::<32>(&mut input)?;
        let unculled = u32::from_le_bytes(read(&mut input)?);
        let mut offsets = [0_u8; 4096 * 4];
        let offset = ((y * 64 + x) * 4) as usize;
        offsets[offset..offset + 4].copy_from_slice(&(12_u32 + 8 + 4096 * 4).to_le_bytes());
        let mut wdl = Vec::new();
        chunk(&mut wdl, b"REVM", &18_u32.to_le_bytes());
        chunk(&mut wdl, b"FOAM", &offsets);
        chunk(&mut wdl, b"ERAM", &heights);
        chunk(&mut wdl, b"OHAM", &masks);
        let low_detail = TerrainLowDetail::decode(&path, &wdl)?;
        assert_eq!(low_detail.tiles().len(), 1);
        let tile = &low_detail.tiles()[0];
        assert_eq!(u32::from(tile.index().x()), x);
        assert_eq!(u32::from(tile.index().y()), y);
        let mesh = TerrainLowDetailMesh::new(tile);
        for (actual, expected) in mesh
            .bounds()
            .into_iter()
            .flatten()
            .zip(bounds.as_chunks::<4>().0)
        {
            assert_eq!(
                actual.to_bits(),
                u32::from_le_bytes(*expected),
                "bounds at {x},{y}"
            );
        }
        for (index, position) in mesh.positions().iter().enumerate() {
            for actual in position {
                assert_eq!(
                    actual.to_bits(),
                    u32::from_le_bytes(read(&mut input)?),
                    "vertex {index} at {x},{y}"
                );
            }
            assert_eq!(u32::from_le_bytes(read(&mut input)?), u32::MAX);
        }
        assert_eq!(mesh.unculled_index_count(), unculled);
        assert_eq!(mesh.indices().len(), 3072);
        for actual in mesh.indices() {
            assert_eq!(
                *actual,
                u16::from_le_bytes(read(&mut input)?),
                "indices at {x},{y}"
            );
        }
    }
    assert_eq!(input.position(), fixture.len() as u64);
    Ok(())
}

/// Reads exact fixture words so a truncated native capture fails immediately.
fn read<const N: usize>(input: &mut impl Read) -> std::io::Result<[u8; N]> {
    let mut bytes = [0; N];
    input.read_exact(&mut bytes)?;
    Ok(bytes)
}

/// Recreates the same controlled archive bytes consumed by the original loader.
fn chunk(output: &mut Vec<u8>, magic: &[u8; 4], payload: &[u8]) {
    output.extend_from_slice(magic);
    output.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    output.extend_from_slice(payload);
}
