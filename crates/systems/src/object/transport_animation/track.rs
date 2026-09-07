//! 70DAA0/70DC10 authored position and quaternion sampling.

use std::num::NonZeroU32;

use glam::Vec3;
use solarity_asset::{TransportAnimationNode, TransportRotationNode};
use solarity_ecs::GameObjectAnimatedPose;
use thiserror::Error;

use crate::object::placement::{compose_rotation, quaternion_matrix};
use crate::object::transport_pose::pack_rotation;
use crate::object::{GameObjectPlacement, GameObjectPlacementError, unpack_game_object_rotation};

/// Authored entry-specific keys and the two retained native interval cursors.
/// Keys retain their stored order, duplicate times, and non-unit quaternions.
pub struct TransportAnimationTrack {
    positions: Box<[TransportAnimationNode]>,
    rotations: Box<[TransportRotationNode]>,
    position_index: usize,
    rotation_index: usize,
    period_ms: Option<NonZeroU32>,
}

/// A sampled local offset and composed world rotation, before quaternion packing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransportAnimationSample {
    /// Parent-rotated offset added to the object's original world position.
    pub offset: Vec3,
    /// Sampled local rotation multiplied by GAMEOBJECT_PARENTROTATION.
    pub rotation: [f32; 4],
    /// Start key's sequence; fewer than two position keys never select a sequence.
    pub sequence_id: Option<u32>,
}

impl TransportAnimationSample {
    /// 711F20/7139E0 publish a matrix rebuilt from the packed sampled quaternion.
    ///
    /// # Errors
    /// Returns a placement error for non-finite or singular sampled inputs.
    pub fn pose(
        self,
        base_position: Vec3,
    ) -> Result<GameObjectAnimatedPose, GameObjectPlacementError> {
        self.pose_with_transport_parent(base_position, None)
    }

    /// 4F43B0 stores the sampled local quaternion before 4F45B0 composes it with
    /// the actual movement parent's rotation. This parent is distinct from the
    /// GAMEOBJECT_PARENTROTATION field already applied during track sampling.
    ///
    /// # Errors
    /// Returns a placement error for non-finite or singular sampled inputs.
    pub fn pose_with_transport_parent(
        self,
        base_position: Vec3,
        transport_parent: Option<[f32; 4]>,
    ) -> Result<GameObjectAnimatedPose, GameObjectPlacementError> {
        if !base_position.is_finite()
            || !self.offset.is_finite()
            || !self.rotation.into_iter().all(f32::is_finite)
        {
            return Err(GameObjectPlacementError::InvalidTransform);
        }
        let packed = pack_rotation(self.rotation);
        let local = unpack_game_object_rotation(packed);
        let placement = GameObjectPlacement::new(
            base_position + self.offset,
            transport_parent.map_or(local, |parent| compose_rotation(local, parent)),
            1.0,
        )?;
        Ok(GameObjectAnimatedPose::new(placement.matrix(), packed))
    }
}

/// Invalid native geometry cannot be replaced by an invented transport pose.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TransportAnimationError {
    /// Native modulo would divide by the zero timestamp of the last position row.
    #[error("transport animation has a zero position period")]
    ZeroPeriod,
    /// Native interval search would cycle indefinitely for this phase and track.
    #[error("transport animation has no {track} interval at phase {phase_ms} ms")]
    MissingInterval {
        /// Position or rotation track whose stored keys do not cover the phase.
        track: &'static str,
        /// Requested phase in milliseconds.
        phase_ms: u32,
    },
    /// A row or current parent quaternion contains a non-finite component.
    #[error("transport animation contains non-finite geometry")]
    NonFinite,
}

impl TransportAnimationTrack {
    /// Takes the entry's contiguous DBC rows without sorting or normalization.
    ///
    /// # Errors
    /// Rejects a zero position period or non-finite authored components. Missing
    /// intervals are diagnosed when sampled rather than repaired or clamped.
    pub fn new(
        positions: &[TransportAnimationNode],
        rotations: &[TransportRotationNode],
    ) -> Result<Self, TransportAnimationError> {
        let period_ms = positions
            .last()
            .map(|node| NonZeroU32::new(node.time_ms).ok_or(TransportAnimationError::ZeroPeriod))
            .transpose()?;
        if positions
            .iter()
            .flat_map(|node| node.position)
            .chain(rotations.iter().flat_map(|node| node.rotation))
            .any(|value| !value.is_finite())
        {
            return Err(TransportAnimationError::NonFinite);
        }
        Ok(Self {
            positions: positions.into(),
            rotations: rotations.into(),
            position_index: 0,
            rotation_index: 0,
            period_ms,
        })
    }

    /// Last position timestamp, also used by the wrapping rotation interval.
    #[must_use]
    pub const fn period_ms(&self) -> Option<NonZeroU32> {
        self.period_ms
    }

    /// 7139E0 skips one loaded-model frame only when both authored arrays are empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.positions.is_empty() && self.rotations.is_empty()
    }

    /// Samples the exact native intervals and parent quaternion multiplication.
    /// `current_rotation` is the GO virtual quaternion, used with fewer than two
    /// rotation keys. A single authored quaternion is deliberately ignored.
    ///
    /// # Errors
    /// Returns missing interval or non-finite geometry errors; never substitutes
    /// a key, extrapolates outside the native intervals, or loops indefinitely.
    pub fn sample(
        &mut self,
        phase_ms: u32,
        parent_rotation: [f32; 4],
        current_rotation: [f32; 4],
    ) -> Result<TransportAnimationSample, TransportAnimationError> {
        if !parent_rotation
            .into_iter()
            .chain(current_rotation)
            .all(f32::is_finite)
        {
            return Err(TransportAnimationError::NonFinite);
        }
        let (offset, sequence_id) = self.position(phase_ms, parent_rotation)?;
        let rotation = self.rotation(phase_ms, parent_rotation, current_rotation)?;
        Ok(TransportAnimationSample {
            offset,
            rotation,
            sequence_id,
        })
    }

    fn position(
        &mut self,
        phase_ms: u32,
        parent: [f32; 4],
    ) -> Result<(Vec3, Option<u32>), TransportAnimationError> {
        if self.positions.len() < 2 {
            return Ok((Vec3::ZERO, None));
        }
        let index = interval(
            self.position_index,
            self.positions.len(),
            phase_ms,
            |index| self.positions[index].time_ms,
            None,
        )
        .ok_or(TransportAnimationError::MissingInterval {
            track: "position",
            phase_ms,
        })?;
        self.position_index = index;
        let first = self.positions[index];
        let second = self.positions[(index + 1) % self.positions.len()];
        // Both interpolated positions and the final matrix products stay in x87
        // until each output component is stored. 4C5100 has the same products
        // and spills as the shared 4C1C40 matrix, with its 3x3 layout compacted.
        let amount = f64::from(fraction(phase_ms, first.time_ms, second.time_ms));
        let blended: [f64; 3] = std::array::from_fn(|axis| {
            f64::from(first.position[axis]) * (1.0 - amount)
                + f64::from(second.position[axis]) * amount
        });
        let matrix = quaternion_matrix(parent).map(f64::from);
        let [x, y, z] = blended;
        let offset = Vec3::from_array(std::array::from_fn(|row| {
            ((z * matrix[8 + row] + y * matrix[4 + row]) + x * matrix[row]) as f32
        }));
        Ok((offset, Some(first.sequence_id)))
    }

    fn rotation(
        &mut self,
        phase_ms: u32,
        parent: [f32; 4],
        current: [f32; 4],
    ) -> Result<[f32; 4], TransportAnimationError> {
        if self.rotations.len() < 2 {
            return Ok(current);
        }
        let index = interval(
            self.rotation_index,
            self.rotations.len(),
            phase_ms,
            |index| self.rotations[index].time_ms,
            self.period_ms.map(NonZeroU32::get),
        )
        .ok_or(TransportAnimationError::MissingInterval {
            track: "rotation",
            phase_ms,
        })?;
        self.rotation_index = index;
        let first = self.rotations[index];
        let second = self.rotations[(index + 1) % self.rotations.len()];
        // 70DC10 substitutes the position period only for interval selection.
        // The fraction denominator still uses the wrapping next.time - current.time.
        Ok(compose_rotation(
            slerp(
                first.rotation,
                second.rotation,
                fraction(phase_ms, first.time_ms, second.time_ms),
            ),
            parent,
        ))
    }
}

fn interval(
    mut current: usize,
    count: usize,
    phase: u32,
    time: impl Fn(usize) -> u32,
    wrap_period: Option<u32>,
) -> Option<usize> {
    for _ in 0..count {
        let next = (current + 1) % count;
        let upper = match time(next) {
            0 => wrap_period.unwrap_or(0),
            time => time,
        };
        if time(current) <= phase && phase < upper {
            return Some(current);
        }
        current = next;
    }
    None
}

fn fraction(phase: u32, first: u32, second: u32) -> f32 {
    (f64::from(phase.wrapping_sub(first)) / f64::from(second.wrapping_sub(first))) as f32
}

/// 982460 uses the shortest arc without normalizing either authored quaternion.
fn slerp(first: [f32; 4], second: [f32; 4], amount: f32) -> [f32; 4] {
    let a = first.map(f64::from);
    let b = second.map(f64::from);
    let dot = ((a[2] * b[2] + a[1] * b[1]) + a[0] * b[0]) + b[3] * a[3];
    let sine = (1.0 - dot * dot).abs().sqrt();
    if sine < 1.0 / 2_097_152.0 {
        return first;
    }
    let angle = sine.atan2(dot.abs());
    let first_weight = ((1.0 - f64::from(amount)) * angle).sin() * (1.0 / sine);
    let second_weight =
        (if dot < 0.0 { -1.0 } else { 1.0 }) * (angle * f64::from(amount)).sin() * (1.0 / sine);
    std::array::from_fn(|axis| (a[axis] * first_weight + b[axis] * second_weight) as f32)
}
