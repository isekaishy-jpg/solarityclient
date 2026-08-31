//! Stock implementation responsibility recovered from `SoundInterface2AdvancedKitProperties.cpp`.

use solarity_asset::AdvancedSoundEntry;

/// Milliseconds in the stock advanced-sound 24-hour schedule.
const DAY_MILLISECONDS: u64 = 86_400_000;

/// Runtime-ready values copied and normalized from one advanced sound row.
///
/// Build 12340 performs these corrections while constructing its live
/// advanced-kit object. Keeping the raw DBC row in the asset crate and the
/// corrected playback values here preserves that same boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdvancedSoundProperties {
    inner_pan_radius: f32,
    outer_pan_radius: f32,
    schedule_milliseconds: [u64; 4],
    duck_gains: [f32; 3],
    inner_influence_radius: f32,
    outer_influence_radius: f32,
    inside_cone_angle: f32,
    outside_cone_angle: f32,
    outside_cone_gain: f32,
}

impl AdvancedSoundProperties {
    /// Returns the stock FMOD 3D-pan blend for a listener and emitter.
    ///
    /// Despite the DBC fields' `2D` suffix, build 12340 measures full XYZ
    /// distance. The inner radius is fully two-dimensional (`0.0`), the outer
    /// radius is fully three-dimensional (`1.0`), and the interval is linear.
    #[must_use]
    pub fn pan_level(self, listener_position: [f32; 3], emitter_position: [f32; 3]) -> f32 {
        if self.outer_pan_radius <= 0.0 {
            return 1.0;
        }

        let distance_squared = squared_distance(listener_position, emitter_position);
        if distance_squared <= self.inner_pan_radius * self.inner_pan_radius {
            return 0.0;
        }
        if self.outer_pan_radius * self.outer_pan_radius < distance_squared {
            return 1.0;
        }

        (distance_squared.sqrt() - self.inner_pan_radius)
            / (self.outer_pan_radius - self.inner_pan_radius)
    }

    /// Returns the rollover-corrected `TimeA` through `TimeD` schedule.
    ///
    /// Each later point that precedes the prior corrected point is moved into
    /// the next 24-hour interval, exactly once, as in the stock constructor.
    #[must_use]
    pub const fn schedule_milliseconds(self) -> [u64; 4] {
        self.schedule_milliseconds
    }

    /// Returns corrected SFX, music, and ambience ducking multipliers.
    ///
    /// Stock replaces any authored value outside the inclusive zero-to-one
    /// interval with `1.0` rather than clamping it to the closest endpoint.
    #[must_use]
    pub const fn duck_gains(self) -> [f32; 3] {
        self.duck_gains
    }

    /// Returns corrected inner and outer duck-influence radii.
    #[must_use]
    pub const fn influence_radii(self) -> [f32; 2] {
        [self.inner_influence_radius, self.outer_influence_radius]
    }

    /// Returns corrected inside and outside FMOD cone angles.
    #[must_use]
    pub const fn cone_angles(self) -> [f32; 2] {
        [self.inside_cone_angle, self.outside_cone_angle]
    }

    /// Returns the authored gain outside the FMOD directional cone.
    #[must_use]
    pub const fn outside_cone_gain(self) -> f32 {
        self.outside_cone_gain
    }
}

impl From<&AdvancedSoundEntry> for AdvancedSoundProperties {
    /// Applies the exact live-object corrections used by build 12340.
    fn from(entry: &AdvancedSoundEntry) -> Self {
        let mut influence_radii = [
            entry.inner_radius_of_influence(),
            entry.outer_radius_of_influence(),
        ];
        if influence_radii[1] < influence_radii[0] {
            influence_radii[0] = influence_radii[1];
        }

        let mut cone_angles = [entry.inside_angle(), entry.outside_angle()];
        if cone_angles[1] < cone_angles[0] {
            cone_angles[0] = cone_angles[1];
        }

        Self {
            inner_pan_radius: entry.inner_radius_2d(),
            outer_pan_radius: entry.outer_radius_2d(),
            schedule_milliseconds: normalize_schedule(entry.times()),
            duck_gains: [
                normalize_duck_gain(entry.duck_to_sfx()),
                normalize_duck_gain(entry.duck_to_music()),
                normalize_duck_gain(entry.duck_to_ambience()),
            ],
            inner_influence_radius: influence_radii[0],
            outer_influence_radius: influence_radii[1],
            inside_cone_angle: cone_angles[0],
            outside_cone_angle: cone_angles[1],
            outside_cone_gain: entry.outside_volume(),
        }
    }
}

/// Rolls decreasing schedule points into the following stock day.
fn normalize_schedule(times: [u32; 4]) -> [u64; 4] {
    let mut normalized = times.map(u64::from);
    for index in 1..normalized.len() {
        if normalized[index] < normalized[index - 1] {
            normalized[index] += DAY_MILLISECONDS;
        }
    }
    normalized
}

/// Preserves the stock neutral fallback for malformed ducking multipliers.
fn normalize_duck_gain(gain: f32) -> f32 {
    if (0.0..=1.0).contains(&gain) {
        gain
    } else {
        1.0
    }
}

/// Computes the exact squared XYZ distance used by the stock update path.
fn squared_distance(first: [f32; 3], second: [f32; 3]) -> f32 {
    let x = first[0] - second[0];
    let y = first[1] - second[1];
    let z = first[2] - second[2];
    x * x + y * y + z * z
}
