//! Camera-dependent native fog policy (7816F0, 7ECD00, 7ECD80, 7F16F0).

use glam::Vec3;

use super::WorldLightSample;

/// Camera far clip and the map's shader-capable fog policy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldFogContext {
    far_clip: f32,
    power: bool,
}

impl WorldFogContext {
    /// Applies 7F16F0's MFOG bank eligibility, portal blend, and liquid exponent.
    /// `base` is camera-clamped scene fog before its camera-liquid multiplier.
    /// `boundary_distance` is absent when no registered group admits indoor fog.
    #[must_use]
    pub fn world_model_scene(
        self,
        base: WorldFogSample,
        palette: crate::WorldModelFogPalette,
        boundary_distance: Option<f32>,
        liquid_flags: Option<u32>,
    ) -> WorldFogSample {
        self.world_model_scene_banks(base, palette, boundary_distance, liquid_flags)[1]
    }

    /// Resolves the ordinary and indoor model fog banks, in that order.
    /// Native 7F16F0 shares the final range/exponent between both banks but
    /// retains the unblended ordinary color, except for forced wet fog.
    #[must_use]
    pub fn world_model_scene_banks(
        self,
        mut base: WorldFogSample,
        palette: crate::WorldModelFogPalette,
        boundary_distance: Option<f32>,
        liquid_flags: Option<u32>,
    ) -> [WorldFogSample; 2] {
        let wet_eligible = liquid_flags.is_some_and(|flags| {
            (flags & 0x20 == 0 || palette.flags() & 0x100 != 0)
                && (flags & 0x100 == 0 || palette.flags() & 0x10 != 0)
        });
        let target = if liquid_flags.is_none() || wet_eligible {
            let bank = palette.banks()[usize::from(wet_eligible)];
            self.world_model(bank.range().0, bank.range().1, bank.color())
        } else {
            base
        };
        if wet_eligible && liquid_flags.is_some_and(|flags| flags & 0x40 != 0) {
            base = target;
        }
        let ordinary_color = base.color;
        if let Some(distance) = boundary_distance {
            // Native retains this product on x87 for the three float blends and
            // only stores the alpha product immediately before integer packing.
            let weight = (f64::from(distance) * f64::from(0.04_f32)).clamp(0., 1.);
            let blend =
                |a: f32, b: f32| ((f64::from(b) - f64::from(a)) * weight + f64::from(a)) as f32;
            base.start = blend(base.start, target.start);
            base.end = blend(base.end, target.end);
            base.exponent = blend(base.exponent, target.exponent);
            let pack = |color: Vec3| {
                let c = color
                    .to_array()
                    .map(|c| (f64::from(c) * 255.).round_ties_even() as u32);
                0xff00_0000 | c[0] << 16 | c[1] << 8 | c[2]
            };
            let alpha = ((weight * 255.) as f32).round_ties_even() as i32 & 255;
            let from = pack(base.color);
            let to = pack(target.color);
            let channel = |shift| {
                let a = ((from >> shift) & 255_u32) as i32;
                let b = ((to >> shift) & 255_u32) as i32;
                if alpha == 255 {
                    b as f32
                } else {
                    (a + (((b - a) * alpha) >> 8)) as f32
                }
            };
            base.color = Vec3::new(channel(16), channel(8), channel(0)) / 255.;
        }
        if self.power && liquid_flags.is_some() {
            base.exponent *= 2.;
        }
        [
            WorldFogSample {
                color: ordinary_color,
                ..base
            },
            base,
        ]
    }

    /// Creates the build-12340 policy for a programmable-shader renderer.
    /// Returns `None` for a nonpositive or nonfinite camera far clip.
    #[must_use]
    pub fn new(map_id: u32, far_clip: f32) -> Option<Self> {
        (far_clip.is_finite() && far_clip > 0.0).then_some(Self {
            far_clip,
            power: map_id >= 530,
        })
    }

    /// Returns whether this map uses the expansion fog curve.
    #[must_use]
    pub const fn uses_power_curve(self) -> bool {
        self.power
    }

    pub(super) fn palette(self, end: f32, ratio: f32) -> (f32, f32, f32) {
        let end = end.max(10.0);
        if !self.power {
            return (end, ratio, 1.0);
        }
        let (end, exponent) = if end >= f32::from_bits(0x41de_38e4) {
            (self.far_clip, self.exponent(ratio * end, end))
        } else {
            (end, 1.0)
        };
        (end, if ratio < 0.0 { 0.0 } else { ratio }, exponent)
    }

    fn exponent(self, start: f32, end: f32) -> f32 {
        let width = f64::from(end) - f64::from(start);
        let reference = f64::from(self.far_clip.min(700.0)) - 200.0;
        if width <= reference {
            ((1.0 - width / reference) * 5.5 + 1.5) as f32
        } else {
            1.5
        }
    }

    fn finish(
        self,
        end: f32,
        ratio: f32,
        exponent: f32,
        color: Vec3,
        camera_in_liquid: bool,
    ) -> WorldFogSample {
        let end = end.min(self.far_clip);
        WorldFogSample {
            start: end * ratio,
            end,
            exponent: exponent
                * if self.power && camera_in_liquid {
                    2.0
                } else {
                    1.0
                },
            color,
        }
    }

    /// Converts one MFOG bank, retaining its absolute start before the end clamp.
    #[must_use]
    pub fn world_model(self, end: f32, ratio: f32, color: Vec3) -> WorldFogSample {
        let end = end.min(self.far_clip);
        let start = ratio * end;
        let end = end.max(30.0);
        if self.power {
            WorldFogSample {
                start: if start < 0.0 { 0.0 } else { start },
                end: self.far_clip,
                exponent: self.exponent(start, end),
                color,
            }
        } else {
            WorldFogSample {
                start,
                end,
                exponent: 1.0,
                color,
            }
        }
    }
}

/// Final camera fog shared by world renderers after palette and camera policy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldFogSample {
    start: f32,
    end: f32,
    exponent: f32,
    color: Vec3,
}

impl WorldFogSample {
    /// Returns the camera-clamped start and end distances.
    #[must_use]
    pub const fn range(self) -> (f32, f32) {
        (self.start, self.end)
    }

    /// Returns the native visibility exponent.
    #[must_use]
    pub const fn exponent(self) -> f32 {
        self.exponent
    }

    /// Returns the final fog RGB.
    #[must_use]
    pub const fn color(self) -> Vec3 {
        self.color
    }
}

impl WorldLightSample {
    /// Resolves 7F16F0's camera clamp and expansion-map camera-liquid exponent.
    #[must_use]
    pub fn final_fog(self, context: WorldFogContext, camera_in_liquid: bool) -> WorldFogSample {
        context.finish(
            self.fog_far,
            self.fog_ratio,
            self.fog_exponent,
            self.fog_color,
            camera_in_liquid,
        )
    }
}

#[cfg(test)]
#[path = "../../../tests/stock_seed/world_fog_native.rs"]
mod tests;
