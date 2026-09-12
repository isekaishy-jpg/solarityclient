//! Original 7816F0's packed exterior-light response across the byte domain.

use super::WorldGlareLighting;
use glam::Vec3;

#[test]
fn exterior_glare_lighting_matches_native_packed_colors() -> Result<(), Box<dyn std::error::Error>>
{
    for row in include_str!("../fixtures/world_glare_lighting_native.txt")
        .lines()
        .filter(|row| !row.starts_with('#'))
    {
        let words = row.split_whitespace().collect::<Vec<_>>();
        let response = f32::from_bits(u32::from_str_radix(words[0], 16)?.swap_bytes());
        let input = u32::from_str_radix(words[1], 16)?;
        let expected = u32::from_str_radix(words[2], 16)?;
        let rgb = Vec3::from_array([16, 8, 0].map(|shift| ((input >> shift) & 255) as f32 / 255.));
        let actual = WorldGlareLighting::new(response).apply(rgb);
        for (channel, shift) in actual.to_array().into_iter().zip([16, 8, 0]) {
            assert_eq!(
                (channel * 255.).round_ties_even() as u32,
                (expected >> shift) & 255,
                "{row}"
            );
        }
    }
    Ok(())
}
