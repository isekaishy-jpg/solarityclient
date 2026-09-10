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
    /// Ordinary FFXGlow, including player blur and the submerged wave clock.
    Normal {
        /// Native DayNight contribution stored in the vertex color's alpha.
        glow: u8,
        /// Native player/water contribution stored in its RGB channels.
        blur: u8,
        /// Present only for the underwater GLOWWAVE pass; uses the client clock.
        wave_time_ms: Option<u32>,
    },
}

impl WorldFrameScreenEffect {
    /// Resolves 4F8770's ordinary glow and the player's unspilled 4F7290 return.
    /// A missing player skips both the inebriation and submerged queries.
    #[must_use]
    pub fn normal(
        glow: f32,
        player_inebriation: Option<f64>,
        submerged: bool,
        milliseconds: u32,
    ) -> Self {
        let wave = player_inebriation.is_some() && submerged;
        Self::Normal {
            glow: normal_byte(f64::from(glow)),
            blur: player_inebriation
                .map_or(0, normal_byte)
                .max(if wave { 0x54 } else { 0 }),
            wave_time_ms: wave.then_some(milliseconds),
        }
    }

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

/// 4F8770 retains the low byte of a shifted float store, rather than FISTP.
fn normal_byte(value: f64) -> u8 {
    (((value * 255. + 512.) as f32).to_bits() >> 14) as u8
}

impl From<WorldFrameGlow> for WorldFrameScreenEffect {
    fn from(glow: WorldFrameGlow) -> Self {
        Self::Glow(glow)
    }
}
