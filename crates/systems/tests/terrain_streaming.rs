//! Original executable evidence for terrain streaming demand.

use std::error::Error;

use glam::Vec3;
use solarity_asset::TerrainTileIndex;
use solarity_ecs::WorldMapId;
use solarity_systems::{
    TerrainStreamingWindow, WorldViewDistanceRequest, prioritize_terrain_tiles,
    resolve_world_view_distance,
};

/// Resolves exact inner/outer chunk ranges from independently captured inputs.
#[test]
fn terrain_streaming_matches_original_chunk_windows() -> Result<(), Box<dyn Error>> {
    let mut count = 0;
    for line in include_str!("fixtures/terrain-streaming-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let name = words.next().ok_or("missing case name")?;
        let mut inputs = [0.0; 7];
        for input in &mut inputs {
            let word = u32::from_str_radix(words.next().ok_or("missing float")?, 16)?;
            *input = f32::from_le_bytes(word.to_be_bytes());
        }
        let origin = Vec3::from_slice(&inputs[..3]);
        let distance = resolve_world_view_distance(WorldViewDistanceRequest::new(
            inputs[3],
            WorldMapId::new(571),
            0x8000_0000,
        ))?;
        assert_eq!(distance.value(), inputs[3]);
        let window = TerrainStreamingWindow::new(origin, distance, Vec3::from_slice(&inputs[4..]))?;
        for value in window
            .required_chunks()
            .into_iter()
            .chain(window.retained_chunks())
            .flatten()
        {
            assert_eq!(
                value,
                words.next().ok_or("missing chunk bound")?.parse::<i32>()?,
                "case {name}"
            );
        }
        assert!(words.next().is_none());
        assert!(window.tiles().all(|tile| window.contains(tile)));
        count += 1;
    }
    assert_eq!(count, 1096);
    Ok(())
}

/// Equal-distance ADTs use the native CRT's observable request order.
#[test]
fn terrain_priority_matches_original_loading_order() -> Result<(), Box<dyn Error>> {
    let mut count = 0;
    for line in include_str!("fixtures/terrain-priority-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let name = words.next().ok_or("missing case name")?;
        let mut origin = [0.; 3];
        for value in &mut origin {
            let word = u32::from_str_radix(words.next().ok_or("missing float")?, 16)?;
            *value = f32::from_le_bytes(word.to_be_bytes());
        }
        let length = words.next().ok_or("missing count")?.parse::<usize>()?;
        let mut tiles = Vec::with_capacity(length);
        for _ in 0..length {
            let x = words.next().ok_or("missing x")?.parse::<u8>()?;
            let y = words.next().ok_or("missing y")?.parse::<u8>()?;
            tiles.push(TerrainTileIndex::new(x, y).ok_or("bad tile")?);
        }
        prioritize_terrain_tiles(&mut tiles, Vec3::from_array(origin))?;
        for tile in tiles {
            let x = words.next().ok_or("missing expected x")?.parse::<u8>()?;
            let y = words.next().ok_or("missing expected y")?.parse::<u8>()?;
            assert_eq!((tile.x(), tile.y()), (x, y), "case {name}");
        }
        assert!(words.next().is_none());
        count += 1;
    }
    assert_eq!(count, 261);
    Ok(())
}
