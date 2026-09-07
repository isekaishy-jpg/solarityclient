//! Native packed RGB/HSV value scaling for liquid depth (7F3230/7ED790).

use glam::Vec3;

use crate::database::sound_environment::LiquidTypeDefinition;

use super::WorldLightSample;

impl WorldLightSample {
    /// Applies the liquid's authored fog, ambient, and direct depth factors.
    /// `depth` is surface minus camera Z, in the liquid query's coordinate basis.
    #[must_use]
    pub fn with_liquid_depth(mut self, liquid: &LiquidTypeDefinition, depth: f32) -> Self {
        let [maximum, fog, ambient, direct] = liquid.darken_parameters();
        if maximum > 0.0 {
            // 7F3674 stores the fraction but retains its extended value for fog;
            // ambient/direct reload that float. Negative depths clamp at the surface.
            let extended_fraction =
                (1.0 - f64::from(depth.clamp(0.0, maximum)) / f64::from(maximum)) - 1.0;
            let fraction = extended_fraction as f32;
            self.fog_color = darken(
                self.fog_color,
                (extended_fraction * f64::from(fog) + 1.0) as f32,
            );
            self.ambient_color = darken(
                self.ambient_color,
                (f64::from(fraction) * f64::from(ambient) + 1.0) as f32,
            );
            self.diffuse_color = darken(
                self.diffuse_color,
                (f64::from(fraction) * f64::from(direct) + 1.0) as f32,
            );
        }
        self
    }
}

/// Retains 7ED790's stored float boundaries and x87 intermediates. Scaling RGB
/// directly differs by an 8-bit channel near rounding boundaries.
fn darken(color: Vec3, factor: f32) -> Vec3 {
    let rgb = color.to_array().map(|value| {
        let byte = (f64::from(value) * 255.0).round_ties_even() as u8;
        f64::from(f32::from(byte) * f32::from_bits(0x3b80_8081))
    });
    // 9829B0 resolves ties toward the later component.
    let largest = if rgb[0] > rgb[1] {
        if rgb[0] > rgb[2] { 0 } else { 2 }
    } else if rgb[1] > rgb[2] {
        1
    } else {
        2
    };
    let value = rgb[largest];
    let delta = value - rgb[0].min(rgb[1]).min(rgb[2]);
    let saturation = if value == 0.0 {
        0.0
    } else {
        (delta / value) as f32
    };
    let scaled = f64::from((value * f64::from(factor)) as f32);
    let converted = if saturation == 0.0 {
        [scaled; 3]
    } else {
        let hue = match largest {
            0 => (rgb[1] - rgb[2]) / delta,
            1 => (rgb[2] - rgb[0]) / delta + 2.0,
            _ => (rgb[0] - rgb[1]) / delta + 4.0,
        } as f32;
        let mut hue = hue * 60.0;
        if hue < 0.0 {
            hue += 360.0;
        }
        if hue >= 360.0 {
            hue -= 360.0;
        }
        let angle = hue * f32::from_bits(0x3c88_8889);
        let sector = ((f64::from(angle) - 0.5).round_ties_even() as i32).min(5);
        let fraction = f64::from(angle) - f64::from(sector);
        let saturation = f64::from(saturation.min(1.0));
        let low = (1.0 - saturation) * scaled;
        let descending = (1.0 - saturation * fraction) * scaled;
        let ascending = (1.0 - (1.0 - fraction) * saturation) * scaled;
        match sector {
            0 => [scaled, ascending, low],
            1 => [descending, scaled, low],
            2 => [low, scaled, ascending],
            3 => [low, descending, scaled],
            4 => [ascending, low, scaled],
            _ => [scaled, low, descending],
        }
    };
    Vec3::from_array(converted.map(|value| {
        // 985030 stores RGB before 9851A0 rounds and keeps each low byte.
        let packed = ((value as f32) * 255.0).round_ties_even() as i32;
        f32::from(packed as u8) / 255.0
    }))
}
