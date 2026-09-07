//! CPU-projected PCT vertices emitted by native 79D5E0.

use glam::{Mat4, Vec3};
use thiserror::Error;

/// Invalid inputs to the native ripple's projected vertex preparation.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error(
    "water ripple vertices require finite geometry and projection and opacity between zero and one"
)]
pub struct WaterRippleVertexError;

/// One world-space ripple vertex with native RGBA and projected texture UV.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterRippleRenderVertex {
    position: [f32; 3],
    color: [u8; 4],
    coordinates: [f32; 2],
}

impl WaterRippleRenderVertex {
    /// Native 79D5E0's packed position, RGBA, and texture coordinates.
    pub const BYTE_SIZE: usize = 24;

    /// Projects one complete retained triangle bank into a reusable frame
    /// buffer. Native 4C21B0 rounds UVs after its complete dot product, while
    /// 79DDBC rounds opacity times 255 before nearest-even integer conversion.
    /// Output order remains unchanged; no clipping or new triangles are added.
    ///
    /// # Errors
    /// Rejects nonfinite input and opacity outside the envelope's normalized
    /// range without changing the output buffer.
    pub fn project_into(
        triangles: &[[Vec3; 3]],
        projection: Mat4,
        opacity: f32,
        output: &mut Vec<Self>,
    ) -> Result<(), WaterRippleVertexError> {
        if !projection.is_finite()
            || !opacity.is_finite()
            || !(0. ..=1.).contains(&opacity)
            || triangles.iter().flatten().any(|point| !point.is_finite())
        {
            return Err(WaterRippleVertexError);
        }
        let matrix = projection.to_cols_array();
        let alpha = (opacity * 255.).round_ties_even() as u8;
        output.reserve(triangles.len().saturating_mul(3));
        for position in triangles.iter().flatten().copied() {
            let coordinates = std::array::from_fn(|index| {
                // Preserve 4C21B0's Z, Y, X, translation accumulation order.
                (((f64::from(position.z) * f64::from(matrix[8 + index])
                    + f64::from(position.y) * f64::from(matrix[4 + index]))
                    + f64::from(position.x) * f64::from(matrix[index]))
                    + f64::from(matrix[12 + index])) as f32
            });
            output.push(Self {
                position: position.to_array(),
                color: [255, 255, 255, alpha],
                coordinates,
            });
        }
        Ok(())
    }

    /// Serializes the exact PCT ABI independently of the Rust host layout.
    #[must_use]
    pub fn to_bytes(self) -> [u8; Self::BYTE_SIZE] {
        let mut bytes = [0; Self::BYTE_SIZE];
        for (value, destination) in self.position.iter().zip(bytes[..12].as_chunks_mut::<4>().0) {
            *destination = value.to_le_bytes();
        }
        bytes[12..16].copy_from_slice(&self.color);
        for (value, destination) in self
            .coordinates
            .iter()
            .zip(bytes[16..].as_chunks_mut::<4>().0)
        {
            *destination = value.to_le_bytes();
        }
        bytes
    }
}
