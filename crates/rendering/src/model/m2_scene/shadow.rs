//! Stock build-12340 M2 shadow constants shared by vertex and pixel stages.

use glam::{Vec2, Vec3, Vec4};

/// One stock 3-by-4 world-to-shadow transform uploaded as three shader rows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2ShadowMatrix {
    rows: [Vec4; 3],
}

impl M2ShadowMatrix {
    /// Creates the three rows used to produce shadow-map x, y, and depth.
    #[must_use]
    pub const fn new(x: Vec4, y: Vec4, depth: Vec4) -> Self {
        Self {
            rows: [x, y, depth],
        }
    }

    /// Returns a disabled transform for a scene without model shadows.
    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            rows: [Vec4::ZERO; 3],
        }
    }

    /// Writes the exact three-register constant layout recovered at VS c224.
    pub(super) fn write_bytes<const N: usize>(self, bytes: &mut [u8; N], offset: &mut usize) {
        for row in self.rows {
            super::gpu_state::write_vec4(bytes, offset, row);
        }
    }
}

/// Per-scene constants consumed by stock's four M2 shadow-map samplers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2ShadowState {
    matrices: [M2ShadowMatrix; 4],
    fade_plane: Vec4,
    light_direction: Vec3,
    filter_offsets: [Vec2; 8],
}

impl M2ShadowState {
    /// Creates one snapshot from stock's four transforms and pixel constants.
    ///
    /// `filter_offsets` are already normalized by the active shadow-map size,
    /// matching the values uploaded to pixel constants c5 through c12.
    #[must_use]
    pub const fn new(
        matrices: [M2ShadowMatrix; 4],
        fade_plane: Vec4,
        light_direction: Vec3,
        filter_offsets: [Vec2; 8],
    ) -> Self {
        Self {
            matrices,
            fade_plane,
            light_direction,
            filter_offsets,
        }
    }

    /// Returns inert constants for permutations that do not sample shadows.
    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            matrices: [M2ShadowMatrix::disabled(); 4],
            fade_plane: Vec4::ZERO,
            light_direction: Vec3::ZERO,
            filter_offsets: [Vec2::ZERO; 8],
        }
    }

    /// Reproduces stock's fixed eight-tap pattern for a square shadow map.
    ///
    /// # Errors
    ///
    /// Returns `None` for a zero-sized map, which stock never creates.
    #[must_use]
    pub fn stock_filter_offsets(texture_size: u32) -> Option<[Vec2; 8]> {
        if texture_size == 0 {
            return None;
        }
        let texel = 1.0 / texture_size as f32;
        Some([
            Vec2::new(0.8, -1.0) * texel,
            Vec2::new(-0.2, -0.8) * texel,
            Vec2::new(0.2, -0.6) * texel,
            Vec2::new(1.0, -0.4) * texel,
            Vec2::new(-0.6, -0.2) * texel,
            Vec2::new(0.6, 0.2) * texel,
            Vec2::new(-1.0, -0.4) * texel,
            Vec2::new(-0.4, -0.6) * texel,
        ])
    }

    /// Writes the exact 352-byte tail appended to the common scene block.
    pub(super) fn write_bytes<const N: usize>(self, bytes: &mut [u8; N], offset: &mut usize) {
        for matrix in self.matrices {
            matrix.write_bytes(bytes, offset);
        }
        super::gpu_state::write_vec4(bytes, offset, self.fade_plane);
        super::gpu_state::write_vec4(bytes, offset, self.light_direction.extend(0.0));
        for filter_offset in self.filter_offsets {
            super::gpu_state::write_vec4(bytes, offset, filter_offset.extend(0.0).extend(0.0));
        }
    }
}
