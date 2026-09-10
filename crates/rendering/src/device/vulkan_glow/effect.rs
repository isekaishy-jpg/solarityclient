//! Authored full-screen effect inputs, independent of swapchain resources.

use super::WorldFrameGlow;

/// The selected postprocessing operation after world draws and before UI draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WorldFrameScreenEffect {
    /// The ModelFFX glow and display-gamma inputs used by Glue scenes.
    Glow(WorldFrameGlow),
    /// FFXDeath's contribution from the quantized DayNight glow field.
    Ghost {
        /// Low byte of the native rounded glow contribution.
        glow: u8,
    },
}

impl WorldFrameScreenEffect {
    /// 7E87B0 stores glow * 255 as a float, rounds to signed integer using the
    /// default x87 mode, then keeps its low byte for the vertex color's alpha.
    #[must_use]
    pub fn ghost(glow: f32) -> Self {
        let scaled = (glow * 255.).round_ties_even();
        let integer = if (-2_147_483_648.0..2_147_483_648.0).contains(&scaled) {
            scaled as i32
        } else {
            i32::MIN
        };
        Self::Ghost {
            glow: integer as u8,
        }
    }
}

impl From<WorldFrameGlow> for WorldFrameScreenEffect {
    fn from(glow: WorldFrameGlow) -> Self {
        Self::Glow(glow)
    }
}
