//! Borrowed native underwater quad bank and immutable per-frame projection.

use glam::{Mat4, Vec3};
use thiserror::Error;

use crate::{BlpTextureHandle, UnderwaterParticleVertex};

/// Invalid underwater projection, fog interval, or native quad/index bank.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("underwater frame requires finite projection/fog and at most 666 complete native quads")]
pub struct UnderwaterParticleFrameError;

/// Native linear vertex fog with the packed D3D fog-color boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UnderwaterParticleFog {
    coefficients: [f32; 2],
    color: Vec3,
}

impl UnderwaterParticleFog {
    /// Captures the native fog interval and rounds normalized RGB to packed bytes.
    ///
    /// # Errors
    /// Rejects nonfinite color/range or a nonpositive fog interval.
    pub fn new(start: f32, end: f32, color: Vec3) -> Result<Self, UnderwaterParticleFrameError> {
        let inverse = 1. / (end - start);
        let offset = end * inverse;
        if !start.is_finite()
            || !end.is_finite()
            || !color.is_finite()
            || end <= start
            || !inverse.is_finite()
            || !offset.is_finite()
        {
            return Err(UnderwaterParticleFrameError);
        }
        let color = Vec3::from_array(color.to_array().map(|value| {
            f32::from((f64::from(value) * 255.).round_ties_even() as i32 as u8) / 255.
        }));
        Ok(Self {
            coefficients: [-inverse, offset],
            color,
        })
    }
}

/// One late-world underwater draw with positive-Z view-space geometry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UnderwaterParticleFrame<'a> {
    projection: Mat4,
    fog: Option<UnderwaterParticleFog>,
    texture: BlpTextureHandle,
    vertices: &'a [UnderwaterParticleVertex],
    indices: &'a [u16],
}

impl<'a> UnderwaterParticleFrame<'a> {
    /// Borrows native billboards. `projection` maps native positive-Z view space
    /// into Vulkan clip coordinates. Native LiquidType flag 16 enables `fog`;
    /// every particle is unlit with white diffuse color.
    ///
    /// # Errors
    /// Rejects nonfinite projection and malformed quad banks.
    pub fn new(
        projection: Mat4,
        fog: Option<UnderwaterParticleFog>,
        texture: BlpTextureHandle,
        vertices: &'a [UnderwaterParticleVertex],
        indices: &'a [u16],
    ) -> Result<Self, UnderwaterParticleFrameError> {
        if !projection.is_finite()
            || vertices.len() > UnderwaterParticleVertex::MAX_QUADS * 4
            || !vertices.len().is_multiple_of(4)
            || indices.len() != vertices.len() / 4 * 6
            || indices
                .iter()
                .any(|index| usize::from(*index) >= vertices.len())
        {
            return Err(UnderwaterParticleFrameError);
        }
        Ok(Self {
            projection,
            fog,
            texture,
            vertices,
            indices,
        })
    }

    /// Reports whether the native quad bank issues a draw.
    #[must_use]
    pub const fn draw_count(self) -> usize {
        (!self.indices.is_empty()) as usize
    }

    pub(in crate::device) const fn texture(self) -> BlpTextureHandle {
        self.texture
    }
    pub(in crate::device) const fn vertices(self) -> &'a [UnderwaterParticleVertex] {
        self.vertices
    }
    pub(in crate::device) const fn indices(self) -> &'a [u16] {
        self.indices
    }

    pub(in crate::device) fn push_bytes(self) -> [u8; 96] {
        let mut bytes = [0; 96];
        let (coefficients, color) = self
            .fog
            .map_or(([0., 1.], Vec3::ZERO), |fog| (fog.coefficients, fog.color));
        for (value, target) in self
            .projection
            .to_cols_array()
            .into_iter()
            .chain([coefficients[0], coefficients[1], 0., 0.])
            .chain(color.extend(1.).to_array())
            .zip(bytes.as_chunks_mut::<4>().0)
        {
            *target = value.to_le_bytes();
        }
        bytes
    }
}
