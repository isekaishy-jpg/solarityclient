use super::*;

#[test]
fn authored_shadow_addresses_and_bits_match_original_client()
-> Result<(), Box<dyn std::error::Error>> {
    let data = include_bytes!("../fixtures/world_entity_terrain_shadow_native.bin");
    let packed: &[u8; 512] = data[..512].try_into()?;
    // Native CPU sampling uses the original last row/column even when the
    // separately uploaded opacity texture has corrected edges.
    let maps = [false, true].map(|edges| TerrainShadowMap::from_packed(packed, edges));
    let (records, tail) = data[512..].as_chunks::<32>();
    assert!(tail.is_empty());
    assert_eq!(records.len(), 650);
    for (case, record) in records.iter().enumerate() {
        let words: Vec<_> = record
            .as_chunks::<4>()
            .0
            .iter()
            .map(|bytes| u32::from_le_bytes(*bytes))
            .collect();
        let point = WorldEntityTerrainShadowPoint::new(Vec3::new(
            f32::from_bits(words[0]),
            f32::from_bits(words[1]),
            0.,
        ));
        if words[2] == u32::MAX {
            assert!(point.is_none(), "outside map case {case}");
            assert_eq!(words[7], 0);
            continue;
        }
        let point = point.ok_or("native admitted point")?;
        assert_eq!(
            [u32::from(point.tile.x()), u32::from(point.tile.y())],
            [words[2], words[3]],
            "tile case {case}"
        );
        assert_eq!(
            u32::from(point.chunk.y()) * 16 + u32::from(point.chunk.x()),
            words[4],
            "chunk case {case}"
        );
        assert_eq!(
            point.texel,
            [words[5] as usize, words[6] as usize],
            "texel case {case}"
        );
        for map in &maps {
            assert_eq!(point.is_shadowed(map), words[7] != 0, "bit case {case}");
        }
    }
    Ok(())
}
