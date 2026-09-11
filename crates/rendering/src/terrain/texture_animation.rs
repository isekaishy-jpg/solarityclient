//! Original world texture clocks (7831A0) and MCLY constants (7D2D70).

use glam::Vec2;

const DIRECTIONS: [[f64; 2]; 8] = [
    [-1.0, 0.0],
    [-1.0, 1.0],
    [0.0, 1.0],
    [1.0, 1.0],
    [1.0, 0.0],
    [1.0, -1.0],
    [0.0, -1.0],
    [-1.0, -1.0],
];
const SPEED_DIVISORS: [f64; 8] = [64.0, 48.0, 32.0, 16.0, 8.0, 4.0, 2.0, 1.0];

/// Persistent world texture animation, shared across terrain tiles and layers.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TerrainTextureAnimationState {
    clocks: [[f32; 2]; 8],
}

impl TerrainTextureAnimationState {
    /// Advances once per world update. Invalid or negative intervals are ignored.
    pub fn advance(&mut self, elapsed_seconds: f32) {
        if !elapsed_seconds.is_finite() || elapsed_seconds < 0.0 {
            return;
        }
        for (clock, direction) in self.clocks.iter_mut().zip(DIRECTIONS) {
            for (component, direction) in clock.iter_mut().zip(direction) {
                // Native x87 compares before the f32 spill. Positive values
                // reset to zero at 64; negative values deliberately keep going.
                let next = f64::from(*component) + direction * f64::from(elapsed_seconds);
                *component = if next >= 64.0 { 0.0 } else { next as f32 };
            }
        }
    }

    /// Returns the diffuse UV translation for an authored MCLY flag word.
    /// Bit 0x40 enables motion; bits 0..2 and 3..5 select direction and speed.
    #[must_use]
    pub fn texture_offset(&self, flags: u32) -> Vec2 {
        if flags & 0x40 == 0 {
            return Vec2::ZERO;
        }
        let clock = self.clocks[(flags & 7) as usize];
        let scale =
            (1.0 / f64::from(0.240_000_01_f32)) / SPEED_DIVISORS[((flags >> 3) & 7) as usize];
        // Terrain.bls adds c13.yx, with a separate register for every layer.
        Vec2::new(
            -(f64::from(clock[1]) * scale * 0.125) as f32,
            -(f64::from(clock[0]) * scale * 0.125) as f32,
        )
    }
}
