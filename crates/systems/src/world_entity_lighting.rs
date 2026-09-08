//! Retained unit ambient transitions and native entity directional callbacks.

use crate::collision::{WorldModelFloorLight, world_model_doodad_light_colors};
use glam::Vec3;

const BYTE_SCALE: f32 = f32::from_bits(0x3b80_8081);
const INTERIOR_RAY: Vec3 = Vec3::new(
    f32::from_bits(0xbe9d_cf03),
    f32::from_bits(0xbe9d_cf03),
    -0.9,
);

/// Coherent daylight inputs for both raw palette and normalized scene-light paths.
#[derive(Clone, Copy, Debug)]
pub struct WorldEntityLightEnvironment {
    ambient: Vec3,
    diffuse: Vec3,
    ray: Vec3,
    palette_ray: Vec3,
}

impl WorldEntityLightEnvironment {
    /// Retains daylight RGB, normalized ray and raw 7EEA90 palette ray.
    #[must_use]
    pub const fn new(ambient: Vec3, diffuse: Vec3, ray: Vec3, palette_ray: Vec3) -> Self {
        Self {
            ambient,
            diffuse,
            ray,
            palette_ray,
        }
    }
}

/// One callback contribution before M2's scene-wide directional finalizer.
#[derive(Clone, Copy, Debug)]
pub struct WorldEntityLightSample {
    ambient: Vec3,
    diffuse: Vec3,
    ray: Vec3,
}

impl WorldEntityLightSample {
    /// Returns the callback's ambient RGB.
    #[must_use]
    pub const fn ambient(self) -> Vec3 {
        self.ambient
    }
    /// Returns its diffuse RGB.
    #[must_use]
    pub const fn diffuse(self) -> Vec3 {
        self.diffuse
    }
    /// Returns its world-space light ray.
    #[must_use]
    pub const fn ray(self) -> Vec3 {
        self.ray
    }
}

/// Native unit color targets, current ambient, and directional intensity.
#[derive(Clone, Copy, Debug)]
pub struct WorldEntityLightState {
    ambient: [u8; 4],
    target_ambient: [u8; 4],
    diffuse: [u8; 4],
    intensity: f32,
    target_intensity: f32,
    interior: bool,
    blend_exterior: bool,
}

impl WorldEntityLightState {
    /// Starts from 781A10's current exterior ambient and unit intensity.
    #[must_use]
    pub fn new(environment: WorldEntityLightEnvironment) -> Self {
        let ambient = pack(environment.ambient);
        Self {
            ambient,
            target_ambient: ambient,
            diffuse: [0; 4],
            intensity: 1.,
            target_intensity: 1.,
            interior: false,
            blend_exterior: false,
        }
    }

    /// Applies 7A1BC0/7A0D60's spatial result. Missing interior MOCV preserves
    /// prior targets; exterior registration supplies the current daylight target.
    pub fn set_floor(
        &mut self,
        interior: bool,
        floor: Option<WorldModelFloorLight>,
        environment: WorldEntityLightEnvironment,
    ) {
        self.interior = interior;
        if !interior {
            self.target_ambient = pack(environment.ambient);
            self.target_intensity = 2.5;
            return;
        }
        if let Some(floor) = floor {
            [self.diffuse, self.target_ambient] = floor.split();
            self.blend_exterior = floor.blends_exterior();
            if self.blend_exterior {
                let alpha = floor.color()[3];
                blend_color(&mut self.diffuse, pack(environment.diffuse), alpha);
                blend_color(&mut self.target_ambient, pack(environment.ambient), alpha);
                self.diffuse[3] = alpha;
            }
        }
        self.target_intensity = if self.blend_exterior {
            (f64::from(self.diffuse[3]) * f64::from(BYTE_SCALE) * 1.5 + 1.) as f32
        } else {
            1.
        };
    }

    /// The renderer's baked-shadow mode uses 7A1BC0's 0.5 exterior target in
    /// authored MCSH shadows. Interior targets are independent of terrain.
    pub fn set_terrain_shadow(&mut self, shadowed: bool) {
        if !self.interior {
            self.target_intensity = if shadowed { 0.5 } else { 2.5 };
        }
    }

    /// Advances 7A1E90 once per scene tick, including its one-byte minimum step.
    pub fn advance(&mut self, seconds: f32, environment: WorldEntityLightEnvironment) {
        let step = ((f64::from(seconds.max(0.)) * 2. * 255.) as i32).max(1);
        let changed = self.ambient[..3] != self.target_ambient[..3];
        for (current, &target) in self.ambient[..3].iter_mut().zip(&self.target_ambient[..3]) {
            let value = i32::from(*current);
            let target = i32::from(target);
            *current = if value < target {
                value.saturating_add(step).min(target)
            } else {
                value.saturating_sub(step).max(target)
            } as u8;
        }
        if !self.interior && !changed {
            self.ambient = pack(environment.ambient);
            self.target_ambient = self.ambient;
        }
        let amount = f64::from(seconds.max(0.)) * f64::from(f32::from_bits(0x4055_5555));
        self.intensity = if self.intensity < self.target_intensity {
            ((f64::from(self.intensity) + amount) as f32).min(self.target_intensity)
        } else if self.intensity > self.target_intensity {
            ((f64::from(self.intensity) - amount) as f32).max(self.target_intensity)
        } else {
            self.intensity
        }
        .min(1.);
    }

    /// Returns 7C1730's contribution using the current transition state.
    #[must_use]
    pub fn sample(self, environment: WorldEntityLightEnvironment) -> WorldEntityLightSample {
        let mut ray = if self.interior {
            INTERIOR_RAY
        } else {
            environment.ray
        };
        let diffuse = if self.interior {
            unpack(self.diffuse)
        } else {
            environment.diffuse
        } * self.intensity;
        if self.interior && self.blend_exterior {
            let amount = f64::from(self.diffuse[3]) * f64::from(BYTE_SCALE);
            ray = Vec3::from_array(std::array::from_fn(|axis| {
                ((f64::from(environment.palette_ray[axis]) - f64::from(INTERIOR_RAY[axis]))
                    * amount
                    + f64::from(INTERIOR_RAY[axis])) as f32
            }));
            let length = ray.as_dvec3().length();
            if length > 0. {
                ray = (ray.as_dvec3() / length).as_vec3();
            }
        }
        WorldEntityLightSample {
            ambient: unpack(self.ambient),
            diffuse,
            ray,
        }
    }

    /// Native MODD colors use 112/96, immediate ambient and the fixed interior ray.
    #[must_use]
    pub fn doodad(
        color: [u8; 4],
        interior: bool,
        environment: WorldEntityLightEnvironment,
    ) -> WorldEntityLightSample {
        if !interior {
            return WorldEntityLightSample {
                ambient: environment.ambient,
                diffuse: environment.diffuse,
                ray: environment.ray,
            };
        }
        let [diffuse, ambient] = world_model_doodad_light_colors(color);
        WorldEntityLightSample {
            ambient: unpack(ambient),
            diffuse: unpack(diffuse),
            ray: INTERIOR_RAY,
        }
    }
}

fn pack(rgb: Vec3) -> [u8; 4] {
    let packed = rgb
        .to_array()
        .map(|value| ((f64::from(value.clamp(0., 1.)) * 255. + 0.5) as u32) as u8);
    [packed[2], packed[1], packed[0], 255]
}

fn unpack(color: [u8; 4]) -> Vec3 {
    Vec3::new(
        f32::from(color[2]) * BYTE_SCALE,
        f32::from(color[1]) * BYTE_SCALE,
        f32::from(color[0]) * BYTE_SCALE,
    )
}

fn blend_color(color: &mut [u8; 4], target: [u8; 4], alpha: u8) {
    for (channel, &target) in color[..3].iter_mut().zip(&target[..3]) {
        *channel = if alpha == 255 {
            target
        } else {
            (i32::from(*channel)
                + (((i32::from(target) - i32::from(*channel)) * i32::from(alpha)) >> 8))
                as u8
        };
    }
}

#[cfg(test)]
#[path = "../tests/stock_seed/world_entity_light_native.rs"]
mod tests;
