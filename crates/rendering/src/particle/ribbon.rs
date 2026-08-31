//! Animated ribbon-emitter values shared by simulation and draw preparation.

use std::collections::VecDeque;

use glam::{Vec3, Vec4};
use solarity_asset::{M2AnimationSet, M2RibbonEmitter};
use thiserror::Error;

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

/// A transformed ribbon spine and its two orientation vectors.
///
/// `width_axis` points toward the authored height-above edge. `tangent`
/// follows the spine and supplies stock's interpolation handles when a frame
/// crosses more than one edge interval.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2RibbonControlPoint {
    center: Vec3,
    width_axis: Vec3,
    tangent: Vec3,
}

impl M2RibbonControlPoint {
    /// Captures one bone-transformed emitter frame.
    #[must_use]
    pub const fn new(center: Vec3, width_axis: Vec3, tangent: Vec3) -> Self {
        Self {
            center,
            width_axis,
            tangent,
        }
    }

    /// Returns the transformed emitter center.
    #[must_use]
    pub const fn center(self) -> Vec3 {
        self.center
    }

    /// Returns the transformed height-above direction.
    #[must_use]
    pub const fn width_axis(self) -> Vec3 {
        self.width_axis
    }

    /// Returns the transformed spine tangent.
    #[must_use]
    pub const fn tangent(self) -> Vec3 {
        self.tangent
    }

    fn is_finite(self) -> bool {
        self.center.is_finite() && self.width_axis.is_finite() && self.tangent.is_finite()
    }
}

/// One retained pair in a placement-local ribbon triangle strip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2RibbonSection {
    above: Vec3,
    below: Vec3,
    color: Vec4,
    age_seconds: f32,
}

impl M2RibbonSection {
    /// Returns the height-above strip vertex.
    #[must_use]
    pub const fn above(self) -> Vec3 {
        self.above
    }

    /// Returns the height-below strip vertex.
    #[must_use]
    pub const fn below(self) -> Vec3 {
        self.below
    }

    /// Returns the color captured when this edge pair was emitted.
    #[must_use]
    pub const fn color(self) -> Vec4 {
        self.color
    }

    /// Returns elapsed lifetime used for expiry, gravity, and texture progress.
    #[must_use]
    pub const fn age_seconds(self) -> f32 {
        self.age_seconds
    }
}

/// Failure while admitting or advancing one stock ribbon trail.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum M2RibbonTrailError {
    /// Authored edge rate is negative, NaN, or infinite.
    #[error("M2 ribbon edge rate must be finite and nonnegative")]
    InvalidEdgeRate,
    /// Authored edge lifetime is NaN or infinite.
    #[error("M2 ribbon edge lifetime must be finite")]
    InvalidEdgeLifetime,
    /// Authored gravity is NaN or infinite.
    #[error("M2 ribbon gravity must be finite")]
    InvalidGravity,
    /// Rate/lifetime multiplication cannot enter process-sized storage.
    #[error("M2 ribbon edge capacity exceeds process limits")]
    EdgeCapacity,
    /// Frame time must be finite and nonnegative before stock's lifetime clamp.
    #[error("M2 ribbon frame delta must be finite and nonnegative")]
    InvalidDelta,
    /// Bone composition produced NaN or infinity.
    #[error("M2 ribbon control point must be finite")]
    InvalidControlPoint,
    /// Animated appearance produced NaN or infinity.
    #[error("M2 ribbon pose must be finite")]
    InvalidPose,
}

/// Mutable edge history owned by exactly one placed ribbon emitter.
#[derive(Debug)]
pub struct M2RibbonTrail {
    sections: VecDeque<M2RibbonSection>,
    previous: Option<M2RibbonControlPoint>,
    edge_rate: f32,
    edge_lifetime_seconds: f32,
    gravity: f32,
    emission_fraction: f32,
    capacity: usize,
    texture_slot: u16,
    has_live_head: bool,
    has_advanced: bool,
}

impl M2RibbonTrail {
    /// Allocates the exact rate/lifetime-derived ring capacity used by stock.
    ///
    /// Build 12340 rounds the edge rate upward, clamps lifetime to 0.25
    /// seconds, then reserves `ceil(rate * lifetime) + 2` edge pairs.
    ///
    /// # Errors
    ///
    /// Returns [`M2RibbonTrailError`] for invalid authored scalars or a
    /// capacity that cannot be represented by this 64-bit process.
    pub fn new(emitter: &M2RibbonEmitter) -> Result<Self, M2RibbonTrailError> {
        let authored_rate = emitter.edges_per_second();
        if !authored_rate.is_finite() || authored_rate < 0.0 {
            return Err(M2RibbonTrailError::InvalidEdgeRate);
        }
        let authored_lifetime = emitter.edge_lifetime_seconds();
        if !authored_lifetime.is_finite() {
            return Err(M2RibbonTrailError::InvalidEdgeLifetime);
        }
        let gravity = emitter.gravity();
        if !gravity.is_finite() {
            return Err(M2RibbonTrailError::InvalidGravity);
        }
        let edge_rate = authored_rate.ceil();
        let edge_lifetime_seconds = authored_lifetime.max(0.25);
        let capacity = (edge_rate * edge_lifetime_seconds).ceil() + 2.0;
        if !capacity.is_finite() || capacity > usize::MAX as f32 {
            return Err(M2RibbonTrailError::EdgeCapacity);
        }
        let capacity = capacity as usize;
        Ok(Self {
            sections: VecDeque::with_capacity(capacity),
            previous: None,
            edge_rate,
            edge_lifetime_seconds,
            gravity,
            emission_fraction: 0.0,
            capacity,
            texture_slot: 0,
            has_live_head: false,
            has_advanced: false,
        })
    }

    /// Advances expiry, gravity, and edge emission for one presentation frame.
    ///
    /// # Errors
    ///
    /// Returns [`M2RibbonTrailError`] for non-finite runtime input.
    pub fn advance(
        &mut self,
        delta_seconds: f32,
        control: M2RibbonControlPoint,
        pose: M2RibbonPose,
    ) -> Result<(), M2RibbonTrailError> {
        if !delta_seconds.is_finite() || delta_seconds < 0.0 {
            return Err(M2RibbonTrailError::InvalidDelta);
        }
        if !control.is_finite() {
            return Err(M2RibbonTrailError::InvalidControlPoint);
        }
        if !pose.is_finite() {
            return Err(M2RibbonTrailError::InvalidPose);
        }

        // The first stock update uses one nominal edge interval. This prevents
        // the first visible frame after construction from depending on a long
        // loader or presentation stall.
        let delta_seconds = if !self.has_advanced && self.edge_rate > 0.0 {
            (self.edge_rate.recip() + 0.000_1).min(self.edge_lifetime_seconds)
        } else {
            delta_seconds.min(self.edge_lifetime_seconds)
        };
        self.has_advanced = true;
        self.texture_slot = pose.texture_slot();

        if !pose.visible() && self.has_live_head {
            self.has_live_head = false;
        }
        self.age_committed_sections(delta_seconds);

        if !pose.visible() {
            self.previous = None;
            self.trim_capacity();
            return Ok(());
        }

        if self.has_live_head {
            self.sections.pop_back();
            self.has_live_head = false;
        }
        let previous = self.previous.unwrap_or(control);
        let total = self.emission_fraction + delta_seconds * self.edge_rate;
        let emitted = total.floor() as usize;
        let interval_span = total - self.emission_fraction;
        for edge in 0..emitted {
            let crossing = edge as f32 + 1.0;
            let amount = if interval_span > 0.0 {
                ((crossing - self.emission_fraction) / interval_span).clamp(0.0, 1.0)
            } else {
                1.0
            };
            self.sections
                .push_back(interpolated_section(previous, control, pose, amount));
        }
        self.emission_fraction = total.fract();
        self.sections.push_back(section(control, pose));
        self.has_live_head = true;
        self.previous = Some(control);
        self.trim_capacity();
        Ok(())
    }

    /// Returns strip sections from the oldest retained pair to the live head.
    #[must_use]
    pub fn sections(&self) -> impl ExactSizeIterator<Item = &M2RibbonSection> {
        self.sections.iter()
    }

    /// Returns stock's rate/lifetime-derived edge-pair capacity.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Returns the most recently sampled flipbook cell.
    #[must_use]
    pub const fn texture_slot(&self) -> u16 {
        self.texture_slot
    }

    fn age_committed_sections(&mut self, delta_seconds: f32) {
        let committed = self
            .sections
            .len()
            .saturating_sub(usize::from(self.has_live_head));
        for section in self.sections.iter_mut().take(committed) {
            let displacement =
                (section.age_seconds * 2.0 + delta_seconds) * self.gravity * delta_seconds;
            section.above.z += displacement;
            section.below.z += displacement;
            section.age_seconds += delta_seconds;
        }
        while self
            .sections
            .front()
            .is_some_and(|section| section.age_seconds > self.edge_lifetime_seconds)
        {
            self.sections.pop_front();
        }
    }

    fn trim_capacity(&mut self) {
        while self.sections.len() > self.capacity {
            self.sections.pop_front();
        }
    }
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
        let sequence = clock.resolve(animations)?;
        let animation_time_ms = clock.animation_time_ms();
        let global_time_ms = clock.global_time_ms();
        let color = sample_vec3(
            animations,
            emitter.color(),
            sequence,
            animation_time_ms,
            global_time_ms,
            Vec3::ONE,
        );
        let alpha = sample_scalar(
            animations,
            emitter.alpha(),
            sequence,
            animation_time_ms,
            global_time_ms,
            1.0,
        );
        Ok(Self {
            color: color.extend(alpha),
            height_above: sample_scalar(
                animations,
                emitter.height_above(),
                sequence,
                animation_time_ms,
                global_time_ms,
                0.0,
            ),
            height_below: sample_scalar(
                animations,
                emitter.height_below(),
                sequence,
                animation_time_ms,
                global_time_ms,
                0.0,
            ),
            texture_slot: sample_discrete(
                animations,
                emitter.texture_slot(),
                sequence,
                animation_time_ms,
                global_time_ms,
                0,
            ),
            visible: sample_discrete(
                animations,
                emitter.visibility(),
                sequence,
                animation_time_ms,
                global_time_ms,
                1,
            ) != 0,
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

    fn is_finite(self) -> bool {
        self.color.is_finite() && self.height_above.is_finite() && self.height_below.is_finite()
    }
}

/// Produces one pair at an exact transformed emitter frame.
fn section(control: M2RibbonControlPoint, pose: M2RibbonPose) -> M2RibbonSection {
    M2RibbonSection {
        above: control.center + control.width_axis * pose.height_above,
        below: control.center - control.width_axis * pose.height_below,
        color: pose.color,
        age_seconds: 0.0,
    }
}

/// Reproduces build 12340's two-handle interpolation between bone frames.
fn interpolated_section(
    previous: M2RibbonControlPoint,
    current: M2RibbonControlPoint,
    pose: M2RibbonPose,
    amount: f32,
) -> M2RibbonSection {
    let previous_section = section(previous, pose);
    let current_section = section(current, pose);
    let inverse = 1.0 - amount;
    let distance = previous.center.distance(current.center);
    let previous_handle = previous.tangent * distance;
    let current_handle = current.tangent * distance;
    M2RibbonSection {
        above: (previous_section.above + previous_handle * amount) * inverse
            + (current_section.above - current_handle * inverse) * amount,
        below: (previous_section.below + previous_handle * amount) * inverse
            + (current_section.below - current_handle * inverse) * amount,
        color: pose.color,
        age_seconds: 0.0,
    }
}
