//! Native previous-frame sun-glare response in exterior ambient/diffuse light.

use glam::Vec3;

/// 7816F0's packed-byte light multiplier; sky, fog and specular remain independent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldGlareLighting {
    multiplier: u8,
}

impl Default for WorldGlareLighting {
    fn default() -> Self {
        Self { multiplier: 255 }
    }
}

impl WorldGlareLighting {
    /// Converts the retained sun response using A3E860's 0.35 scale.
    pub(crate) fn new(response: f32) -> Self {
        let attenuation = (f64::from(response) * f64::from(0.35_f32)) as f32;
        let multiplier = ((1. - f64::from(attenuation)) * 255.) as f32;
        Self {
            multiplier: multiplier.round_ties_even() as i32 as u8,
        }
    }

    /// Applies stock's byte multiplication to a normalized exterior light color.
    #[must_use]
    pub fn apply(self, color: Vec3) -> Vec3 {
        Vec3::from_array(color.to_array().map(|channel| {
            let byte = (f64::from(channel) * 255.).round_ties_even() as i32 as u8;
            let scaled = (u32::from(byte) * u32::from(self.multiplier) + 255) >> 8;
            scaled as f32 * (1. / 255.)
        }))
    }
}

#[cfg(test)]
#[path = "../../../tests/stock_seed/glare_lighting_native.rs"]
mod tests;
