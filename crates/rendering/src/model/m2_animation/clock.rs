//! Sequence clocks and the previous sequence retained during a native blend.

use solarity_asset::{M2AnimationSet, M2Interpolation, M2Track};

use super::M2BonePoseError;

/// The local animation and global clocks used by model tracks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2AnimationClock {
    sequence: usize,
    animation_time_ms: f32,
    global_time_ms: f32,
    secondary: Option<SecondaryClock>,
}

/// One previous sequence, sampled independently of the new primary timer.
#[derive(Clone, Copy, Debug, PartialEq)]
struct SecondaryClock {
    sequence: usize,
    animation_time_ms: f32,
    weight: f32,
}

impl M2AnimationClock {
    /// Creates one snapshot; track consumers validate times and availability.
    #[must_use]
    pub const fn new(sequence: usize, animation_time_ms: f32, global_time_ms: f32) -> Self {
        Self {
            sequence,
            animation_time_ms,
            global_time_ms,
            secondary: None,
        }
    }

    /// Retains a previous sequence for continuous, non-global track blending.
    ///
    /// A weight of one selects the previous pose; zero selects the primary.
    /// Track consumers reject non-finite times and weights outside `[0, 1]`.
    /// Native step and discrete tracks continue to select the primary value.
    #[must_use]
    pub const fn with_secondary_sequence(
        mut self,
        sequence: usize,
        animation_time_ms: f32,
        weight: f32,
    ) -> Self {
        self.secondary = Some(SecondaryClock {
            sequence,
            animation_time_ms,
            weight,
        });
        self
    }

    /// Returns the selected zero-based sequence before alias resolution.
    #[must_use]
    pub const fn sequence(self) -> usize {
        self.sequence
    }

    /// Returns elapsed time on the selected sequence clock.
    #[must_use]
    pub const fn animation_time_ms(self) -> f32 {
        self.animation_time_ms
    }

    /// Returns elapsed time on the shared global animation clock.
    #[must_use]
    pub const fn global_time_ms(self) -> f32 {
        self.global_time_ms
    }

    /// Validates both clocks and replaces aliases with their payload slots.
    pub(crate) fn resolve(mut self, animations: &M2AnimationSet) -> Result<Self, M2BonePoseError> {
        if !self.animation_time_ms.is_finite() || !self.global_time_ms.is_finite() {
            return Err(M2BonePoseError::NonFiniteTime);
        }
        self.sequence = resolve_sequence(animations, self.sequence)?;
        if let Some(secondary) = self.secondary.as_mut() {
            if !secondary.animation_time_ms.is_finite() {
                return Err(M2BonePoseError::NonFiniteTime);
            }
            if !(0.0..=1.0).contains(&secondary.weight) {
                return Err(M2BonePoseError::InvalidBlendWeight);
            }
            secondary.sequence = resolve_sequence(animations, secondary.sequence)?;
        }
        Ok(self)
    }

    /// 0x0082B0A0/0x0082AF40 skip secondary sampling for step and global tracks.
    pub(super) fn secondary_for<T>(self, track: &M2Track<T>) -> Option<(Self, f32)> {
        if track.interpolation() == M2Interpolation::Step || track.global_sequence().is_some() {
            return None;
        }
        let secondary = self.secondary.filter(|secondary| secondary.weight != 0.0)?;
        Some((
            Self::new(
                secondary.sequence,
                secondary.animation_time_ms,
                self.global_time_ms,
            ),
            secondary.weight,
        ))
    }
}

/// Resolves the authored payload without inventing another available sequence.
fn resolve_sequence(
    animations: &M2AnimationSet,
    sequence: usize,
) -> Result<usize, M2BonePoseError> {
    // Models without sequence records still use their implicit zero channel.
    if animations.sequences().is_empty() && sequence == 0 {
        return Ok(0);
    }
    if sequence >= animations.sequences().len() {
        return Err(M2BonePoseError::SequenceIndex {
            requested: sequence,
            available: animations.sequences().len(),
        });
    }
    if animations.is_sequence_available(sequence) != Some(true) {
        return Err(M2BonePoseError::SequenceUnavailable { sequence });
    }
    animations
        .resolve_sequence_alias(sequence)
        .ok_or(M2BonePoseError::SequenceIndex {
            requested: sequence,
            available: animations.sequences().len(),
        })
}
