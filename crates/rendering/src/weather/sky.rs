//! Build 12340's fixed sky dome and sun-facing packed-color gradient.

use glam::{Mat4, Vec3, Vec4};

/// A retained dome and its camera-relative transform for one world submission.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldSkyFrame<'a> {
    dome: &'a WorldSkyDome,
    view_projection: Mat4,
}

impl<'a> WorldSkyFrame<'a> {
    /// Uses the validated world camera with translation removed and native scale.
    #[must_use]
    pub fn new(dome: &'a WorldSkyDome, camera: crate::WorldCameraFrame) -> Self {
        let mut rotation = camera.view();
        rotation.w_axis = Vec4::W;
        Self {
            dome,
            view_projection: camera.projection()
                * rotation
                * Mat4::from_scale(Vec3::splat(6.666_666_5)),
        }
    }

    /// Returns the retained source mesh for this frame.
    #[must_use]
    pub const fn dome(self) -> &'a WorldSkyDome {
        self.dome
    }

    /// Returns the camera-relative native sky transform.
    #[must_use]
    pub const fn view_projection(self) -> Mat4 {
        self.view_projection
    }
}

/// The original dome has two poles and five 24-vertex latitude rings.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldSkyDome {
    positions: [[f32; 3]; 122],
    colors: [u32; 122],
    indices: [u16; 300],
}

impl Default for WorldSkyDome {
    fn default() -> Self {
        Self::new()
    }
}

impl WorldSkyDome {
    /// Constructs the executable's unit-radius mesh once for reuse each frame.
    #[must_use]
    pub fn new() -> Self {
        Self::with_radius(1.0)
    }

    fn with_radius(radius: f32) -> Self {
        const LATITUDES: [f32; 7] = [0.0, 0.17, 0.2, 0.23, 0.24, 0.25, 1.0];
        let mut dome = Self {
            positions: [[0.0; 3]; 122],
            colors: [0xff00_0000; 122],
            indices: [0; 300],
        };
        let offset = -(0.785_398_185_253_143_3_f64.cos() as f32);
        let mut vertex = 0;
        let mut previous_start = 0;
        for (ring, latitude) in LATITUDES.into_iter().enumerate() {
            let start = vertex;
            let angle = f64::from(latitude) * f64::from(std::f32::consts::PI);
            let phase = angle * f64::from(0.318_309_87_f32);
            let horizontal = periodic(phase - 0.5) as f32;
            let vertical = periodic(phase) as f32;
            let z = (f64::from(vertical) * f64::from(radius) + f64::from(offset)) as f32;
            for segment in 0..if ring == 0 || ring == 6 { 1 } else { 24 } {
                let longitude =
                    segment as f64 * f64::from(1.0_f32 / 24.0) * f64::from(std::f32::consts::TAU);
                dome.positions[vertex] = [
                    (longitude.sin() * f64::from(horizontal) * f64::from(radius)) as f32,
                    (longitude.cos() * f64::from(horizontal) * f64::from(radius)) as f32,
                    z,
                ];
                vertex += 1;
            }
            if ring > 0 {
                for segment in 0..25 {
                    let index = (ring - 1) * 50 + segment * 2;
                    dome.indices[index] =
                        (previous_start + if ring == 1 { 0 } else { segment % 24 }) as u16;
                    dome.indices[index + 1] =
                        (start + if ring == 6 { 0 } else { segment % 24 }) as u16;
                }
            }
            previous_start = start;
        }
        dome
    }

    /// Replaces the complete gradient from the five sky bands and final fog color.
    /// Time is the normalized day; the final camera supplies native view azimuth.
    pub fn update_colors(
        &mut self,
        sky: [Vec3; 5],
        fog: Vec3,
        day: f32,
        highlight: f32,
        camera: crate::WorldCameraFrame,
    ) {
        self.update_packed(
            sky.map(pack),
            pack(fog),
            day,
            highlight,
            view_azimuth(camera.forward()),
        );
    }

    fn update_packed(&mut self, sky: [u32; 5], fog: u32, day: f32, highlight: f32, azimuth: f32) {
        const DAY: [[f32; 2]; 6] = [
            [0.125, 0.0],
            [0.270_833_34, 1.0],
            [0.291_666_7, 0.0],
            [0.854_166_6, 0.0],
            [0.895_833_3, 1.0],
            [0.999_305_55, 0.0],
        ];
        const AROUND: [[f32; 2]; 6] = [
            [0.125, 1.0],
            [0.375, 0.0],
            [0.5, -0.5],
            [0.625, -0.7],
            [0.75, -0.5],
            [0.875, 0.0],
        ];
        let strength = (cyclic(&DAY, day) * f64::from(highlight)) as f32;
        self.colors[0] = sky[0]; // RGB -> HSV -> RGB at value multiplier one.
        for ring in 1..5 {
            let highlight_color = blend(sky[ring], sky[1], strength);
            let mut phase = f64::from(azimuth) * f64::from(0.159_154_94_f32) + 0.25;
            if phase > 1.0 {
                phase -= 1.0;
            }
            for segment in 0..24 {
                if phase < 0.0 {
                    phase += 1.0;
                }
                let facing = cyclic(&AROUND, phase as f32);
                let color = if facing >= 0.0 {
                    blend(
                        sky[ring],
                        highlight_color,
                        ((1.0 - facing) * f64::from(strength)) as f32,
                    )
                } else {
                    let dark = blend(highlight_color, sky[0], strength * 0.7);
                    blend(highlight_color, dark, -(facing as f32) * strength)
                };
                self.colors[1 + (ring - 1) * 24 + segment] = color;
                phase = f64::from(phase as f32) - f64::from(1.0_f32 / 24.0);
            }
        }
        self.colors[97..].fill(fog);
    }

    /// Executable-owned positions, relative to the sky origin.
    #[must_use]
    pub const fn positions(&self) -> &[[f32; 3]; 122] {
        &self.positions
    }

    /// Native opaque ARGB words, in the same order as the vertices.
    #[must_use]
    pub const fn colors(&self) -> &[u32; 122] {
        &self.colors
    }

    /// Six consecutive 50-index strips, including their repeated seam vertices.
    #[must_use]
    pub const fn indices(&self) -> &[u16; 300] {
        &self.indices
    }
}

fn periodic(phase: f64) -> f64 {
    let period = if phase <= 0.0 {
        phase as i32 - 1
    } else {
        phase as i32
    };
    let fraction = phase - f64::from(period);
    let value = 1.0 - (6.0 - 4.0 * fraction) * fraction * fraction;
    if period & 1 != 0 { -value } else { value }
}

fn view_azimuth(forward: Vec3) -> f32 {
    let [x, y, z] = forward.to_array().map(f64::from);
    let mut angle = if x * x + y * y > 0.0001 {
        y.atan2(x)
    } else {
        z.atan2(x)
    };
    if angle < 0.0 {
        angle += f64::from(std::f32::consts::TAU);
    }
    angle as f32
}

pub(super) fn cyclic(table: &[[f32; 2]], phase: f32) -> f64 {
    let phase = f64::from(phase.clamp(0.0, 1.0));
    let right = table
        .iter()
        .position(|key| phase < f64::from(key[0]))
        .unwrap_or(0);
    let left = (right + table.len() - 1) % table.len();
    let mut width = f64::from(table[right][0]) - f64::from(table[left][0]);
    if width < 0.0 {
        width += 1.0;
    }
    let mut elapsed = phase - f64::from(table[left][0]);
    if elapsed < 0.0 {
        elapsed += 1.0;
    }
    let first = f64::from(table[left][1]);
    first + (f64::from(table[right][1]) - first) * (elapsed / width)
}

fn blend(first: u32, second: u32, weight: f32) -> u32 {
    let mut color = 0xff00_0000;
    for shift in [0, 8, 16] {
        let a = f64::from((first >> shift) & 255);
        let b = f64::from((second >> shift) & 255);
        let channel = (a + (b - a) * f64::from(weight)) as f32;
        color |= (((f64::from(channel) - 0.5).round_ties_even() as i32 as u32) & 255) << shift;
    }
    color
}

fn pack(color: Vec3) -> u32 {
    let [r, g, b] = color
        .to_array()
        .map(|v| (v * 255.0).round_ties_even() as u32);
    0xff00_0000 | r << 16 | g << 8 | b
}

#[cfg(test)]
#[path = "../../tests/stock_seed/sky_native.rs"]
mod tests;
