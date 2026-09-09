//! Original Terrain.bls projection captures at large world coordinates.

use std::error::Error;

use glam::{Mat4, Vec3, Vec4};
use solarity_rendering::{M2LocalLightState, M2SceneUniform, TerrainSceneUniform};

/// Reads the serialized matrix that the actual shader descriptor consumes.
fn matrix(bytes: &[u8]) -> Mat4 {
    let (words, _) = bytes[..64].as_chunks::<4>();
    Mat4::from_cols_array(&std::array::from_fn(|index| {
        f32::from_le_bytes(words[index])
    }))
}

/// Stock view-then-projection preserves depth during orbit far from the origin.
#[test]
fn world_projection_preserves_original_shader_depth_at_large_coordinates()
-> Result<(), Box<dyn Error>> {
    let mut count = 0;
    let mut combined_failures = 0;
    for line in include_str!("../fixtures/world_projection_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let mut fields = line.split_whitespace();
        let name = fields.next().ok_or("missing native projection case")?;
        let values = fields
            .map(|word| u32::from_str_radix(word, 16).map(f32::from_bits))
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(values.len(), 37);
        let point = Vec3::from_slice(&values[..3]).extend(1.0);
        let projection = Mat4::from_cols_array(values[3..19].try_into()?);
        let view = Mat4::from_cols_array(values[19..35].try_into()?);
        let native_depth = values[35];
        let native_reciprocal_w = values[36];
        let terrain =
            TerrainSceneUniform::new(projection, view, Vec3::ONE, Vec3::ZERO, Vec3::Z).to_bytes();
        let model = M2SceneUniform::new(
            projection,
            view,
            Vec3::ZERO,
            Vec3::ONE,
            Vec3::ZERO,
            Vec3::Z,
            Vec4::ZERO,
            Vec3::ZERO,
            [M2LocalLightState::disabled(); 4],
        )
        .to_bytes();
        for (bytes, view_offset) in [(&terrain[..], 160), (&model[..], 784)] {
            let clip = matrix(bytes) * (matrix(&bytes[view_offset..]) * point);
            assert!(
                (clip.z / clip.w - native_depth).abs() <= 2.0 * f32::EPSILON,
                "{name}: depth {} differs from stock {native_depth}",
                clip.z / clip.w
            );
            assert!(
                (clip.w.recip() - native_reciprocal_w).abs() <= 2.0e-6,
                "{name}: reciprocal W differs from stock"
            );
        }
        let combined = (projection * view) * point;
        combined_failures +=
            usize::from((combined.z / combined.w - native_depth).abs() > 16.0 * f32::EPSILON);
        count += 1;
    }
    assert_eq!(count, 72);
    // Keep cases that expose the old combined-matrix cancellation, so a future
    // fixture edit cannot silently reduce this to origin-only camera coverage.
    assert!(combined_failures > 10);
    Ok(())
}
