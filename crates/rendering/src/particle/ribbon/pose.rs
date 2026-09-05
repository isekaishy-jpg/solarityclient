//! Immutable animated values sampled from one M2 ribbon declaration.

use glam::{Vec3, Vec4};
use solarity_asset::{M2AnimationSet, M2RibbonEmitter};

use crate::model::m2_animation::sample::{sample_discrete, sample_scalar, sample_vec3};
use crate::{M2AnimationClock, M2BonePoseError};

/// One immutable sample of an authored ribbon declaration.
///
/// Mutable edge history belongs to each placed M2. This value contains only
/// the animation result that can be copied into newly emitted edge pairs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2RibbonPose {
    color: Vec4,
    height_above: f32,
    height_below: f32,
    texture_slot: u16,
    visible: bool,
}

impl M2RibbonPose {
    /// Samples the emitter through the same local/global sequence routing used
    /// by bones and M2 materials.
    ///
    /// # Errors
    ///
    /// Returns [`M2BonePoseError`] when the selected sequence or either clock
    /// is unavailable to the decoded model.
    pub fn sample(
        animations: &M2AnimationSet,
        emitter: &M2RibbonEmitter,
        clock: M2AnimationClock,
    ) -> Result<Self, M2BonePoseError> {
        let clock = clock.resolve(animations)?;
        let color = sample_vec3(animations, emitter.color(), clock, Vec3::ONE);
        let alpha = sample_scalar(animations, emitter.alpha(), clock, 1.0);
        Ok(Self {
            color: color.extend(alpha),
            height_above: sample_scalar(animations, emitter.height_above(), clock, 0.0),
            height_below: sample_scalar(animations, emitter.height_below(), clock, 0.0),
            texture_slot: sample_discrete(animations, emitter.texture_slot(), clock, 0),
            visible: sample_discrete(animations, emitter.visibility(), clock, 1) != 0,
        })
    }

    /// Returns authored linear RGB and signed-fixed16 opacity.
    #[must_use]
    pub const fn color(self) -> Vec4 {
        self.color
    }

    /// Returns the edge distance above the transformed emitter center.
    #[must_use]
    pub const fn height_above(self) -> f32 {
        self.height_above
    }

    /// Returns the edge distance below the transformed emitter center.
    #[must_use]
    pub const fn height_below(self) -> f32 {
        self.height_below
    }

    /// Returns the held flipbook cell selector.
    #[must_use]
    pub const fn texture_slot(self) -> u16 {
        self.texture_slot
    }

    /// Reports whether the byte-valued emitter visibility key is nonzero.
    #[must_use]
    pub const fn visible(self) -> bool {
        self.visible
    }

    pub(super) fn is_finite(self) -> bool {
        self.color.is_finite() && self.height_above.is_finite() && self.height_below.is_finite()
    }
}
