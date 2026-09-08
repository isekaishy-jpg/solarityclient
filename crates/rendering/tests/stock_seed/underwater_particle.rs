//! Captured original 79CA70 billboard loop, without replacement math hooks.

use std::error::Error;

use glam::Mat4;
use solarity_rendering::UnderwaterParticleVertex;

#[test]
fn underwater_billboards_match_original_vertices_and_indices() -> Result<(), Box<dyn Error>> {
    let fixture = include_bytes!("../fixtures/underwater-particle-draw.bin");
    let mut remaining = fixture.as_slice();
    let mut word = || {
        let (bytes, rest) = remaining.split_at(4);
        remaining = rest;
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
    };
    let count = word();
    assert_eq!(count, 52);
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for case in 0..count {
        let pattern = word();
        let input_count = word();
        let vertex_count = word() as usize;
        let matrix = std::array::from_fn(|_| f32::from_bits(word()));
        let particles: Vec<[f32; 4]> = (0..input_count)
            .map(|_| std::array::from_fn(|_| f32::from_bits(word())))
            .collect();
        UnderwaterParticleVertex::project_into(
            &particles,
            Mat4::from_cols_array(&matrix),
            pattern,
            &mut vertices,
            &mut indices,
        )?;
        assert_eq!(vertices.len(), vertex_count, "case {case}");
        for (index, vertex) in vertices.iter().enumerate() {
            let expected: [u8; 24] = std::array::from_fn::<_, 6, _>(|_| word())
                .map(u32::to_le_bytes)
                .as_flattened()
                .try_into()?;
            assert_eq!(vertex.to_bytes(), expected, "case {case}, vertex {index}");
        }
        let expected: Vec<u16> = (0..vertex_count / 4 * 3)
            .flat_map(|_| {
                let value = word();
                [value as u16, (value >> 16) as u16]
            })
            .collect();
        assert_eq!(indices, expected, "case {case} indices");
    }
    assert!(remaining.is_empty());
    Ok(())
}
