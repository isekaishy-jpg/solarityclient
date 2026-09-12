//! 7EF6E0's retained visibility, angular size and packed opacity.

use super::{WorldGlareEnvironment, WorldGlareKind};
use crate::weather::sky::cyclic;
use glam::Vec3;

/// One process-retained native glare visibility accumulator.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct GlareState {
    visibility: f32,
    lighting_response: f32,
}

/// Updated native draw properties and query admission before angular opacity.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GlareSample {
    pub(crate) size: f32,
    pub(crate) color: u32,
    pub(crate) query: bool,
}

impl GlareState {
    /// Previous-frame 7EF6E0 output consumed by 7816F0 before world lighting.
    pub(crate) const fn lighting_response(self) -> f32 {
        self.lighting_response
    }

    /// Applies native factors and asymmetric fade rates. The completed GPU
    /// sample ratio is a backend input; missing results retain the last count.
    #[allow(clippy::too_many_arguments)] // Each value crosses a distinct native input boundary.
    pub(crate) fn update(
        &mut self,
        kind: WorldGlareKind,
        environment: WorldGlareEnvironment,
        ray: Vec3,
        disc_size: f32,
        forward: Vec3,
        color: u32,
        occlusion: f32,
    ) -> GlareSample {
        let (table, cloud, rise, scale, sizes, alphas) = match kind {
            WorldGlareKind::Sun => (
                [[0.270_833_34, 0.], [0.3125, 1.], [0.8125, 1.], [0.875, 0.]],
                1. - f64::from(environment.cloud_alpha[0]),
                4.,
                1_f32,
                [3_f32, 20.],
                [0.5, 1.],
            ),
            WorldGlareKind::Moon => (
                [
                    [0.083_333_336, 1.],
                    [0.135_416_67, 0.],
                    [0.947_916_7, 0.],
                    [0.999_305_6, 1.],
                ],
                1. - ((f64::from(environment.cloud_alpha[1]) - 0.5) * 2.).abs(),
                3.030_303_f32,
                2.,
                // 7EECC0 replaces both moon angular endpoints with its current disc size.
                [disc_size, disc_size],
                [0.1_f32, 1.],
            ),
        };
        let cloud = cloud as f32;
        let water = environment.liquid_depth.map_or(1., |depth| {
            1. - (f64::from(depth) * f64::from(0.1_f32)).clamp(0., 1.)
        });
        let target =
            ((1. - f64::from(environment.skybox_weight)) * water * f64::from(cloud)) as f32;
        let target = (cyclic(&table, environment.day) * f64::from(target)) as f32;
        let query = target != 0. && color >> 24 != 0;
        let target = if target != 0. {
            (f64::from(target) * f64::from(if color >> 24 == 0 { 0. } else { occlusion })) as f32
        } else {
            target
        };
        if self.visibility < target {
            self.visibility = (f64::from(rise) * f64::from(environment.elapsed_seconds)
                + f64::from(self.visibility))
            .min(f64::from(target)) as f32;
        } else if self.visibility > target {
            self.visibility = (f64::from(self.visibility)
                - f64::from(1.515_151_5_f32) * f64::from(environment.elapsed_seconds))
            .max(f64::from(target)) as f32;
        }
        let [x, y, z] = ray.to_array().map(f64::from);
        let inverse = 1. / (y * y + z * z + x * x).sqrt();
        let direction = [x, y, z].map(|v| (v * inverse) as f32);
        let dot = f64::from(forward.y) * f64::from(direction[1])
            + f64::from(forward.z) * f64::from(direction[2])
            + f64::from(forward.x) * f64::from(direction[0]);
        let angular =
            (dot.max(f64::from(0.7_f32)) - f64::from(0.7_f32)) / (1. - f64::from(0.7_f32));
        // The final pow uses the stored, clamped dot, not the remapped angle.
        self.lighting_response = (f64::from(dot.max(f64::from(0.7_f32)) as f32).powi(10)
            * f64::from(self.visibility)) as f32;
        let size = ((f64::from(sizes[1]) - f64::from(sizes[0])) * angular + f64::from(sizes[0]))
            * f64::from(scale);
        let alpha = (((f64::from(alphas[1]) - f64::from(alphas[0])) * angular
            + f64::from(alphas[0]))
            * f64::from(self.visibility)
            * (f64::from(color >> 24) * f64::from(1. / 255.0_f32))
            * 255.) as f32;
        GlareSample {
            size: size as f32,
            color: (color & 0x00ff_ffff) | ((alpha.round_ties_even() as u32 & 255) << 24),
            query,
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/stock_seed/glare_native.rs"]
mod tests;
