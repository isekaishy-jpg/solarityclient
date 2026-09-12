//! Native glare sampling reads cloud alpha independently of displayed BGRA banks.

use super::WorldClouds;
use glam::Vec3;

/// Decode exact little-endian oracle storage.
#[allow(clippy::unwrap_used)]
fn float(word: &str) -> f32 {
    f32::from_le_bytes(std::array::from_fn(|i| {
        u8::from_str_radix(&word[i * 2..i * 2 + 2], 16).unwrap()
    }))
}

/// A patterned bank makes projection, truncation and texel selection observable.
#[test]
fn glare_cloud_opacity_matches_original_projection() {
    let mut clouds = WorldClouds::new(1);
    for y in 0..128 {
        for x in 0..128 {
            clouds.alpha[y * 128 + x] = ((x * 13 + y * 37) & 255) as u8;
        }
    }
    for row in include_str!("../fixtures/world_glare_cloud_native.txt")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let values: Vec<_> = row.split_whitespace().map(float).collect();
        let actual = clouds.opacity_at(
            Vec3::from_slice(&values[..3]),
            Vec3::from_slice(&values[3..6]),
        );
        assert_eq!(actual.to_bits(), values[6].to_bits(), "{row}");
    }
}
