//! Stock implementation responsibility recovered from `SoundInterface2AdvancedKitProperties.cpp`.

use solarity_asset::AdvancedSoundEntry;

/// Milliseconds in the stock advanced-sound 24-hour schedule.
const DAY_MILLISECONDS: i64 = 86_400_000;

/// Runtime-ready values copied and normalized from one advanced sound row.
///
/// Build 12340 performs these corrections while constructing its live
/// advanced-kit object. Keeping the raw DBC row in the asset crate and the
/// corrected playback values here preserves that same boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdvancedSoundProperties {
    inner_pan_radius: f32,
    outer_pan_radius: f32,
    schedule_milliseconds: [i64; 4],
    random_offset_range: i32,
    usage: u32,
    repeat_interval_milliseconds: [i32; 2],
    duck_gains: [f32; 3],
    inner_influence_radius: f32,
    outer_influence_radius: f32,
    inside_cone_angle: f32,
    outside_cone_angle: f32,
    outside_cone_gain: f32,
    duck_transition_milliseconds: [i32; 2],
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

    /// Evaluates the stock daily volume envelope at one realm-day time.
    ///
    /// `day_milliseconds` is the cyclic value in `0..86_400_000` produced from
    /// the authoritative game clock. `random_offset_milliseconds` is selected
    /// once for the live advanced object from the authored symmetric range.
    /// Stock applies that offset to window comparisons but not to the ramp
    /// numerators; this method intentionally preserves that asymmetry.
    ///
    /// `None` means the current time is outside a nonempty authored window.
    /// Four zero time fields leave the initial gain at `1.0` for the full day.
    #[must_use]
    pub fn scheduled_gain(
        self,
        day_milliseconds: u32,
        random_offset_milliseconds: i32,
    ) -> Option<f32> {
        let [time_a, time_b, time_c, time_d] = self.schedule_milliseconds;
        let offset = i64::from(random_offset_milliseconds);
        let mut current = i64::from(day_milliseconds);

        // A window crossing midnight compares its early-day samples in the
        // following integer day, matching the stock update routine.
        if time_d + offset >= DAY_MILLISECONDS && current < time_a + offset {
            current += DAY_MILLISECONDS;
        }

        let has_window = self.schedule_milliseconds != [0; 4];
        if has_window && (current < time_a + offset || time_d + offset < current) {
            return None;
        }
        if !has_window {
            return Some(1.0);
        }

        let gain = if current < time_b + offset {
            ratio(current - time_a, time_b - time_a)
        } else if current < time_c + offset {
            1.0
        } else {
            1.0 - ratio(current - time_c, time_d - time_c)
        };
        Some(gain.clamp(0.0, 1.0))
    }

    /// Returns the rollover-corrected `TimeA` through `TimeD` schedule.
    ///
    /// Each later point that precedes the prior corrected point is moved into
    /// the next 24-hour interval, exactly once, as in the stock constructor.
    #[must_use]
    pub const fn schedule_milliseconds(self) -> [i64; 4] {
        self.schedule_milliseconds
    }

    /// Returns the signed range used for the per-instance schedule offset.
    ///
    /// The DBC stores a 32-bit word, while build 12340 reads the field as a
    /// signed integer and selects from `-range..range`.
    #[must_use]
    pub const fn random_offset_range(self) -> i32 {
        self.random_offset_range
    }

    /// Returns the raw usage word consumed by the advanced lifecycle.
    ///
    /// Interpretation remains in the media layer because the asset layer must
    /// preserve the exact DBC value without inventing enum semantics.
    #[must_use]
    pub const fn usage(self) -> u32 {
        self.usage
    }

    /// Returns the signed minimum and maximum repeat intervals.
    #[must_use]
    pub const fn repeat_interval_milliseconds(self) -> [i32; 2] {
        self.repeat_interval_milliseconds
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

    /// Returns the authored duck and unduck transition durations.
    ///
    /// Build 12340 consumes both DBC words as signed milliseconds.
    #[must_use]
    pub const fn duck_transition_milliseconds(self) -> [i32; 2] {
        self.duck_transition_milliseconds
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
            random_offset_range: entry.random_offset_range() as i32,
            usage: entry.usage(),
            repeat_interval_milliseconds: [
                entry.time_interval_minimum() as i32,
                entry.time_interval_maximum() as i32,
            ],
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
            duck_transition_milliseconds: [
                entry.time_to_duck() as i32,
                entry.time_to_unduck() as i32,
            ],
        }
    }
}

/// Rolls decreasing schedule points into the following stock day.
fn normalize_schedule(times: [u32; 4]) -> [i64; 4] {
    let mut normalized = times.map(i64::from);
    for index in 1..normalized.len() {
        if normalized[index] < normalized[index - 1] {
            normalized[index] += DAY_MILLISECONDS;
        }
    }
    normalized
}

/// Performs the stock signed-integer-to-float division used by envelope ramps.
fn ratio(numerator: i64, denominator: i64) -> f32 {
    numerator as f32 / denominator as f32
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
