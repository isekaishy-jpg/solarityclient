//! Stock linear particle-lifetime ramp evaluation.

use glam::{Vec2, Vec3, Vec4};
use solarity_asset::{M2ParticleEmitter, M2ParticleLifetimeTrack};
use thiserror::Error;

use super::M2ParticleRandom;

/// WotLK's normalized particle-lifetime key corresponding to death.
const LIFETIME_KEY_MAXIMUM: f32 = i16::MAX as f32;

/// Uses independent random multipliers for the two authored scale axes.
const INDEPENDENT_SCALE_VARIATION: u32 = 0x0080_0000;

/// Selects a random head atlas cell when the lifetime ramp has no keys.
const RANDOM_HEAD_TEXTURE_CELL: u32 = 0x0010_0000;

/// Executable lower bound at `0x009E8CD0` for a random scale multiplier.
const MINIMUM_SCALE_MULTIPLIER: f32 = f32::from_bits(0x38d1_b717);

/// One render-state sample at a particle's normalized age.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2ParticleLifetimePose {
    color: Vec4,
    scale: Vec2,
    head_texture_cell: u32,
    tail_texture_cell: u32,
}

impl M2ParticleLifetimePose {
    /// Samples continuous ramps linearly and holds discrete atlas selectors.
    ///
    /// Build 12340 stores lifetime timestamps in a `u16` container but reads
    /// them as signed fixed-16 values. `0x0000..=0x7FFF` therefore maps birth
    /// through death. Ages outside the live range are clamped to its endpoints
    /// so a render prepared on the death update uses the final authored keys.
    ///
    /// # Errors
    ///
    /// Returns [`M2ParticleLifetimePoseError::NonFiniteAge`] when the supplied
    /// normalized age is NaN or infinite.
    pub fn sample(
        emitter: &M2ParticleEmitter,
        normalized_age: f32,
        random_word: u16,
    ) -> Result<Self, M2ParticleLifetimePoseError> {
        if !normalized_age.is_finite() {
            return Err(M2ParticleLifetimePoseError::NonFiniteAge);
        }
        let key = normalized_age.clamp(0.0, 1.0) * LIFETIME_KEY_MAXIMUM;
        let color = sample_linear(emitter.color(), key, Vec3::ONE, Vec3::lerp);
        let alpha = sample_linear(emitter.alpha(), key, 1.0, |from, to, amount| {
            from + (to - from) * amount
        });
        let mut random = M2ParticleRandom::new(u32::from(random_word));
        let head_texture_cell = sample_held(emitter.head_uv_animation(), key, 0).map_or_else(
            || {
                if emitter.flags() & RANDOM_HEAD_TEXTURE_CELL == 0 {
                    0
                } else {
                    random_atlas_cell(emitter, &mut random)
                }
            },
            u32::from,
        );
        let mut scale = sample_linear(emitter.scale(), key, Vec2::ONE, Vec2::lerp);
        let variation = emitter.scale_variation();
        if emitter.flags() & INDEPENDENT_SCALE_VARIATION != 0 {
            scale.x *= (1.0 + random.next_signed() * variation.x).max(MINIMUM_SCALE_MULTIPLIER);
            scale.y *= (1.0 + random.next_signed() * variation.y).max(MINIMUM_SCALE_MULTIPLIER);
        } else {
            let multiplier =
                (1.0 + random.next_signed() * variation.x).max(MINIMUM_SCALE_MULTIPLIER);
            scale *= multiplier;
        }
        Ok(Self {
            color: color.extend(alpha),
            scale,
            head_texture_cell,
            tail_texture_cell: sample_held(emitter.tail_uv_animation(), key, 0)
                .map_or(0, u32::from),
        })
    }

    /// Returns authored linear RGB and signed-fixed16 opacity.
    #[must_use]
    pub const fn color(self) -> Vec4 {
        self.color
    }

    /// Returns the particle's two authored billboard scale axes.
    #[must_use]
    pub const fn scale(self) -> Vec2 {
        self.scale
    }

    /// Returns the held head flipbook-cell selector.
    #[must_use]
    pub const fn head_texture_cell(self) -> u32 {
        self.head_texture_cell
    }

    /// Returns the held tail flipbook-cell selector.
    #[must_use]
    pub const fn tail_texture_cell(self) -> u32 {
        self.tail_texture_cell
    }
}

/// A particle lifetime cannot be mapped into the stock finite key domain.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum M2ParticleLifetimePoseError {
    /// The normalized age was NaN or infinite.
    #[error("particle normalized age must be finite")]
    NonFiniteAge,
}

/// Locates the two authored keys surrounding one stock normalized-life key.
fn interval<T>(track: &M2ParticleLifetimeTrack<T>, key: f32) -> Option<(usize, usize, f32)> {
    let timestamps = track.timestamps();
    if timestamps.is_empty() {
        return None;
    }
    let upper = timestamps.partition_point(|timestamp| f32::from(*timestamp) <= key);
    let lower = upper.saturating_sub(1).min(timestamps.len() - 1);
    let upper = (lower + 1).min(timestamps.len() - 1);
    let start = f32::from(timestamps[lower]);
    let end = f32::from(timestamps[upper]);
    let amount = if end > start {
        ((key - start) / (end - start)).clamp(0.0, 1.0)
    } else {
        0.0
    };
    Some((lower, upper, amount))
}

/// Evaluates one continuous lifetime ramp without manufacturing missing keys.
fn sample_linear<T>(
    track: &M2ParticleLifetimeTrack<T>,
    key: f32,
    default: T,
    interpolate: fn(T, T, f32) -> T,
) -> T
where
    T: Copy,
{
    let Some((lower, upper, amount)) = interval(track, key) else {
        return default;
    };
    let Some(&from) = track.values().get(lower) else {
        return default;
    };
    let Some(&to) = track.values().get(upper) else {
        return from;
    };
    interpolate(from, to, amount)
}

/// Samples an atlas selector as a held integer rather than a fractional cell.
fn sample_held<T>(track: &M2ParticleLifetimeTrack<T>, key: f32, default: T) -> Option<T>
where
    T: Copy,
{
    let (lower, _upper, _amount) = interval(track, key)?;
    Some(track.values().get(lower).copied().unwrap_or(default))
}

/// Reduces one raw generator word into the complete authored atlas domain.
fn random_atlas_cell(emitter: &M2ParticleEmitter, random: &mut M2ParticleRandom) -> u32 {
    let cell_count = u32::from(emitter.texture_rows()) * u32::from(emitter.texture_columns());
    ((u64::from(random.next_u32()) * u64::from(cell_count)) >> 32) as u32
}
