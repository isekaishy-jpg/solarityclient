//! Borrowed native circular/directional vertex banks at one camera sample.

use glam::Mat4;
use thiserror::Error;

use crate::{BlpTextureHandle, WaterRippleRenderVertex};

/// One of the native ripple texture passes, retaining active-record order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterRipplePass<'a> {
    texture: BlpTextureHandle,
    vertices: &'a [WaterRippleRenderVertex],
}

impl<'a> WaterRipplePass<'a> {
    /// Borrows projected triangles and their renderer-owned texture.
    #[must_use]
    pub const fn new(texture: BlpTextureHandle, vertices: &'a [WaterRippleRenderVertex]) -> Self {
        Self { texture, vertices }
    }

    /// Returns the native low-16-bit submitted vertex count from 79D5E0.
    /// Full vertices are retained even when this count wraps or becomes zero.
    #[must_use]
    pub const fn draw_vertex_count(self) -> u32 {
        self.vertices.len() as u16 as u32
    }

    pub(in crate::device) const fn texture(self) -> BlpTextureHandle {
        self.texture
    }

    pub(in crate::device) const fn vertices(self) -> &'a [WaterRippleRenderVertex] {
        self.vertices
    }
}

/// Invalid camera, bias, or incomplete triangle data for a surface-ripple frame.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("water ripple frame requires finite camera/bias and complete triangle banks")]
pub struct WaterRippleFrameError;

/// Two native texture passes inserted immediately after transparent water.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterRippleFrame<'a> {
    view_projection: Mat4,
    depth_bias: f32,
    water_scene_order: u32,
    passes: [Option<WaterRipplePass<'a>>; 2],
}

impl<'a> WaterRippleFrame<'a> {
    /// Captures the camera, `footstepBias` CVar, and ordered circular/directional
    /// banks. Stock's CVar default is 0.125; its device bias is negative.
    ///
    /// # Errors
    /// Rejects nonfinite transforms/bias or banks that are not whole triangles.
    pub fn new(
        view_projection: Mat4,
        footstep_bias: f32,
        water_scene_order: u32,
        circular: Option<WaterRipplePass<'a>>,
        directional: Option<WaterRipplePass<'a>>,
    ) -> Result<Self, WaterRippleFrameError> {
        let passes = [circular, directional];
        let depth_bias = -(footstep_bias * f32::from_bits(0x3a80_0080));
        if !view_projection.is_finite()
            || !depth_bias.is_finite()
            || passes.iter().flatten().any(|pass| {
                !pass.vertices.len().is_multiple_of(3)
                    || u32::try_from(pass.vertices.len()).is_err()
            })
        {
            return Err(WaterRippleFrameError);
        }
        Ok(Self {
            view_projection,
            depth_bias,
            water_scene_order,
            passes,
        })
    }

    /// Returns the number of native passes with a nonzero submitted count.
    #[must_use]
    pub fn draw_count(self) -> usize {
        self.passes
            .iter()
            .flatten()
            .filter(|pass| pass.draw_vertex_count() != 0)
            .count()
    }

    pub(in crate::device) fn vertex_count(self) -> usize {
        self.passes
            .iter()
            .flatten()
            .map(|pass| pass.vertices.len())
            .sum()
    }

    pub(in crate::device) const fn passes(self) -> [Option<WaterRipplePass<'a>>; 2] {
        self.passes
    }

    pub(in crate::device) const fn water_scene_order(self) -> u32 {
        self.water_scene_order
    }

    /// Inserts this frame at the compositor's resolved transparent-water ordinal.
    #[must_use]
    pub const fn with_water_scene_order(mut self, order: u32) -> Self {
        self.water_scene_order = order;
        self
    }

    pub(in crate::device) fn push_bytes(self) -> [u8; 68] {
        let mut bytes = [0; 68];
        for (value, word) in self
            .view_projection
            .to_cols_array()
            .iter()
            .zip(bytes[..64].as_chunks_mut::<4>().0)
        {
            *word = value.to_le_bytes();
        }
        bytes[64..].copy_from_slice(&self.depth_bias.to_le_bytes());
        bytes
    }
}
