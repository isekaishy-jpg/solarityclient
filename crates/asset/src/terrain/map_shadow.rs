//! Strict build-12340 MCSH expansion into shader-ready opacity texels.

/// Width and height of one authored terrain shadow plane.
pub const TERRAIN_SHADOW_MAP_WIDTH: usize = 64;

/// Byte count of the expanded single-channel shadow payload.
pub const TERRAIN_SHADOW_MAP_BYTE_COUNT: usize =
    TERRAIN_SHADOW_MAP_WIDTH * TERRAIN_SHADOW_MAP_WIDTH;

const PACKED_SHADOW_MAP_BYTE_COUNT: usize = TERRAIN_SHADOW_MAP_BYTE_COUNT / 8;
// 7B87F0 expands set bits to zero visibility (lookup A4004C), including
// both 16-bit and 32-bit material textures. The pixel shader owns strength.
const AUTHORED_SHADOW_OPACITY: u8 = 255;

/// One decoded 64-by-64 terrain shadow-opacity plane.
pub struct TerrainShadowMap {
    opacity: Box<[u8; TERRAIN_SHADOW_MAP_BYTE_COUNT]>,
    authored: Box<[u8; PACKED_SHADOW_MAP_BYTE_COUNT]>,
}

impl TerrainShadowMap {
    /// Expands the LSB-first MCSH bitmap and applies stock's edge rule.
    ///
    /// `do_not_fix_edges` corresponds to MCNK flag `0x8000`, shared with the
    /// terrain alpha-map path.
    #[must_use]
    pub fn from_packed(bytes: &[u8; PACKED_SHADOW_MAP_BYTE_COUNT], do_not_fix_edges: bool) -> Self {
        let mut opacity = Box::new([0_u8; TERRAIN_SHADOW_MAP_BYTE_COUNT]);
        for (texel, destination) in opacity.iter_mut().enumerate() {
            let packed = bytes[texel / 8];
            if packed & (1 << (texel % 8)) != 0 {
                *destination = AUTHORED_SHADOW_OPACITY;
            }
        }
        if !do_not_fix_edges {
            fix_shadow_edges(&mut opacity);
        }
        Self {
            opacity,
            authored: Box::new(*bytes),
        }
    }

    /// Returns the stable shader-ready R8 payload.
    #[must_use]
    pub fn opacity(&self) -> &[u8; TERRAIN_SHADOW_MAP_BYTE_COUNT] {
        &self.opacity
    }

    /// Reads the original MCSH bit, before the GPU edge correction.
    /// Native entity lighting queries this authored bitmap rather than opacity.
    #[must_use]
    pub fn authored_shadow_at(&self, column: usize, row: usize) -> Option<bool> {
        if column >= TERRAIN_SHADOW_MAP_WIDTH || row >= TERRAIN_SHADOW_MAP_WIDTH {
            return None;
        }
        Some(self.authored[row * 8 + column / 8] & (1 << (column & 7)) != 0)
    }
}

fn fix_shadow_edges(values: &mut [u8; TERRAIN_SHADOW_MAP_BYTE_COUNT]) {
    for row in 0..TERRAIN_SHADOW_MAP_WIDTH - 1 {
        let final_column = row * TERRAIN_SHADOW_MAP_WIDTH + TERRAIN_SHADOW_MAP_WIDTH - 1;
        values[final_column] = values[final_column - 1];
    }
    let previous_row = (TERRAIN_SHADOW_MAP_WIDTH - 2) * TERRAIN_SHADOW_MAP_WIDTH;
    let final_row = (TERRAIN_SHADOW_MAP_WIDTH - 1) * TERRAIN_SHADOW_MAP_WIDTH;
    values.copy_within(
        previous_row..previous_row + TERRAIN_SHADOW_MAP_WIDTH,
        final_row,
    );
}
