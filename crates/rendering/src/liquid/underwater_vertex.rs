//! Camera-relative billboards from native 79CA70, including its fixed atlas bank.

use glam::Mat4;
use thiserror::Error;

/// Invalid geometry, camera rotation, or LiquidType texture pattern.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error(
    "underwater billboards require finite geometry and camera, and texture pattern 0 through 4"
)]
pub struct UnderwaterParticleVertexError;

/// One native positive-Z view-space PCT vertex.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UnderwaterParticleVertex {
    position: [f32; 3],
    coordinates: [f32; 2],
}

const PATTERNS: [[usize; 8]; 5] = [
    [0, 1, 2, 3, 4, 5, 6, 7],
    [8, 9, 10, 11, 8, 9, 10, 11],
    [12; 8],
    [13, 14, 15, 16, 13, 14, 15, 16],
    [17, 18, 19, 20, 21, 22, 23, 24],
];

impl UnderwaterParticleVertex {
    /// Native packed XYZ, white RGBA, UV stride.
    pub const BYTE_SIZE: usize = 24;
    /// The original transient vertex buffer holds at most 666 complete quads.
    pub const MAX_QUADS: usize = 666;

    /// Replaces reusable output banks with 79CA70's admitted quads and indices.
    /// Input XYZ positions are relative to the eye; matrix translation is ignored.
    /// The matrix's forward axis must point along positive view-space Z.
    ///
    /// # Errors
    /// Invalid inputs leave both output banks unchanged.
    pub fn project_into(
        particles: &[[f32; 4]],
        native_view: Mat4,
        texture_pattern: u32,
        vertices: &mut Vec<Self>,
        indices: &mut Vec<u16>,
    ) -> Result<(), UnderwaterParticleVertexError> {
        let Some(pattern) = PATTERNS.get(texture_pattern as usize) else {
            return Err(UnderwaterParticleVertexError);
        };
        if !native_view.is_finite() || particles.iter().flatten().any(|value| !value.is_finite()) {
            return Err(UnderwaterParticleVertexError);
        }
        vertices.clear();
        indices.clear();
        let quads = particles.len().min(Self::MAX_QUADS);
        vertices.reserve(quads * 4);
        indices.reserve(quads * 6);
        let m = native_view.to_cols_array().map(f64::from);
        let mut slot = 8;
        for (index, particle) in particles.iter().enumerate() {
            let [px, py, pz, size] = particle.map(f64::from);
            // x87 keeps X/Y/Z extended through the five strict admission tests.
            // Y's accumulation is Z+X+Y; the other two rows use Z+Y+X.
            let x = (pz * m[8] + py * m[4]) + px * m[0];
            let y = (pz * m[9] + px * m[1]) + py * m[5];
            let z = (pz * m[10] + py * m[6]) + px * m[2];
            if z > 0. && -z < x && x < z && -z < y && y < z {
                let base = vertices.len() as u16;
                let (column, row) = match slot {
                    0..=11 => (slot % 4, slot / 4),
                    12..=16 => (4, slot - 12),
                    _ => ((slot - 17) % 4, 3 + (slot - 17) / 4),
                };
                for (dx, dy, du, dv) in [
                    (-0.5, 0.5, 0, 0),
                    (-0.5, -0.5, 0, 1),
                    (0.5, 0.5, 1, 0),
                    (0.5, -0.5, 1, 1),
                ] {
                    vertices.push(Self {
                        position: [(x + dx * size) as f32, (y + dy * size) as f32, z as f32],
                        coordinates: [
                            (column + du) as f32 * (51. / 256.),
                            (row + dv) as f32 * (51. / 256.),
                        ],
                    });
                }
                indices.extend([base, base + 1, base + 2, base + 3, base + 2, base + 1]);
                if vertices.len() == Self::MAX_QUADS * 4 {
                    break;
                }
            }
            // The first particle uses slot 8. Even rejected particles advance this bank.
            slot = pattern[index & 7];
        }
        Ok(())
    }

    /// Serializes the original PCT ABI independently of the Rust host layout.
    #[must_use]
    pub fn to_bytes(self) -> [u8; Self::BYTE_SIZE] {
        let mut bytes = [255; Self::BYTE_SIZE];
        for (value, target) in self.position.iter().zip(bytes[..12].as_chunks_mut::<4>().0) {
            *target = value.to_le_bytes();
        }
        for (value, target) in self
            .coordinates
            .iter()
            .zip(bytes[16..].as_chunks_mut::<4>().0)
        {
            *target = value.to_le_bytes();
        }
        bytes
    }
}
