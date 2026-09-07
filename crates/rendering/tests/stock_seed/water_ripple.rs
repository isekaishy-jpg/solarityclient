//! Ripple presentation fixtures execute the original 4C3290 and 7E2D60.

use std::error::Error;

use glam::{Mat4, Vec3};
use solarity_rendering::{WaterRippleRenderVertex, water_ripple_surface_transform};

/// Native matrix stores include yaw quadrants and large-coordinate bounds rounding.
#[test]
fn water_ripple_projection_matches_original_matrix_instructions() -> Result<(), Box<dyn Error>> {
    let fixture = include_bytes!("../fixtures/water_ripple_projection.bin");
    assert_eq!(fixture.len(), 262 * 84);
    for (case, record) in fixture.as_chunks::<84>().0.iter().enumerate() {
        let values: Vec<_> = record
            .as_chunks::<4>()
            .0
            .iter()
            .map(|bytes| f32::from_le_bytes(*bytes))
            .collect();
        let projection =
            water_ripple_surface_transform(Vec3::from_slice(&values[..3]), values[3], values[4])?;
        for (index, (actual, expected)) in projection
            .to_cols_array()
            .iter()
            .zip(&values[5..])
            .enumerate()
        {
            assert_eq!(
                actual.to_bits(),
                expected.to_bits(),
                "case {case}, element {index}: {actual} != {expected}; {:?}",
                &values[..5]
            );
        }
    }
    Ok(())
}

/// Native CPU UV evaluation and FISTP alpha rounding preserve every packed byte.
#[test]
fn water_ripple_vertices_match_original_projection_and_alpha() -> Result<(), Box<dyn Error>> {
    let fixture = include_bytes!("../fixtures/water_ripple_vertices.bin");
    assert_eq!(fixture.len(), 7860 * 92);
    let mut vertices = Vec::new();
    for (case, record) in fixture.as_chunks::<92>().0.iter().enumerate() {
        let words: Vec<_> = record
            .as_chunks::<4>()
            .0
            .iter()
            .map(|bytes| f32::from_le_bytes(*bytes))
            .collect();
        let matrix: [f32; 16] = words[..16].try_into()?;
        let point = Vec3::from_slice(&words[17..20]);
        vertices.clear();
        WaterRippleRenderVertex::project_into(
            &[[point; 3]],
            Mat4::from_cols_array(&matrix),
            words[16],
            &mut vertices,
        )?;
        assert_eq!(vertices.len(), 3);
        for vertex in &vertices {
            assert_eq!(vertex.to_bytes(), record[68..92], "case {case}");
        }
    }
    Ok(())
}
