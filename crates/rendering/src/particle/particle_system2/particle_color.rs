//! Placement-local `ParticleColor.dbc` substitution for M2 emitters.

use glam::Vec3;
use solarity_asset::{ParticleColorCatalog, ParticleColorDefinition};

/// First M2 particle-color selector replaced by display metadata.
const FIRST_REPLACEMENT_SELECTOR: u16 = 11;

/// Number of selector columns in the exact build-12340 DBC row.
const REPLACEMENT_SELECTOR_COUNT: usize = 3;

/// Stock's diagnostic color for a missing nonzero `ParticleColorID`.
const MISSING_PARTICLE_COLOR: u32 = 0xff00_ff00;

/// Three replacement ramps applied to one independently placed M2.
///
/// Build 12340 does not mutate the shared M2 asset. `M2Model` copies the
/// selected display colors into the placed emitter systems whose authored
/// selectors are 11, 12, or 13. Keeping this value beside placement state
/// preserves that ownership when many displays share one decoded model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2ParticleColorReplacement {
    ramps: [[Vec3; 3]; REPLACEMENT_SELECTOR_COUNT],
}

impl M2ParticleColorReplacement {
    /// Resolves stock's optional display-level replacement.
    ///
    /// Identifier zero performs no setter calls. A missing nonzero identifier
    /// uses the exact green diagnostic value passed by `Wow.exe` at
    /// `0x004EA9E0`; this is an observed stock fallback rather than a guessed
    /// substitute.
    #[must_use]
    pub fn resolve(catalog: &ParticleColorCatalog, id: u32) -> Option<Self> {
        if id == 0 {
            return None;
        }
        Some(catalog.definition(id).map_or_else(Self::missing, Self::new))
    }

    /// Transposes one DBC row into the three `SetParticleColor` calls.
    #[must_use]
    pub fn new(definition: &ParticleColorDefinition) -> Self {
        let start = definition.start();
        let middle = definition.middle();
        let end = definition.end();
        Self {
            ramps: std::array::from_fn(|index| {
                [
                    decode_argb_rgb(start[index]),
                    decode_argb_rgb(middle[index]),
                    decode_argb_rgb(end[index]),
                ]
            }),
        }
    }

    /// Returns the replacement ramp for authored selectors 11 through 13.
    #[must_use]
    pub const fn ramp(self, selector: u16) -> Option<[Vec3; 3]> {
        let Some(index) = selector.checked_sub(FIRST_REPLACEMENT_SELECTOR) else {
            return None;
        };
        if index as usize >= REPLACEMENT_SELECTOR_COUNT {
            return None;
        }
        Some(self.ramps[index as usize])
    }

    fn missing() -> Self {
        let color = decode_argb_rgb(MISSING_PARTICLE_COLOR);
        Self {
            ramps: [[color; 3]; REPLACEMENT_SELECTOR_COUNT],
        }
    }
}

/// Converts DBC `0xAARRGGBB` to the renderer's normalized linear RGB tuple.
fn decode_argb_rgb(color: u32) -> Vec3 {
    let red = ((color >> 16) & 0xff) as f32;
    let green = ((color >> 8) & 0xff) as f32;
    let blue = (color & 0xff) as f32;
    Vec3::new(red, green, blue) / 255.0
}
