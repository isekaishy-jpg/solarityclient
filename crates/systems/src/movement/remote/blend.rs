//! Native lookahead interpolation from `006EA6A0` and `006EA7E0`.

#[cfg(test)]
#[path = "../../../tests/movement/remote_blend.rs"]
mod tests;

use glam::Vec3;
use solarity_ecs::WorldTransform;

const EPSILON: f64 = f32::from_bits(0x3580_0000) as f64;
const FULL_TURN: f64 = f32::from_bits(0x40c9_0fdb) as f64;
const HALF_TURN: f64 = f32::from_bits(0x4049_0fdb) as f64;

/// Position and angles in the subject's current movement coordinate space.
#[derive(Clone, Copy, Debug)]
pub struct RemoteMovementPose {
    /// Ground/world or parent-relative transform, according to attachment.
    pub transform: WorldTransform,
    /// Native retained pitch, including zero outside pitching modes.
    pub pitch: f32,
    /// Zero for world space, otherwise the owning transport GUID.
    pub transport_guid: u64,
}

/// Lookahead corrections retained until the first queued command changes.
#[derive(Clone, Debug)]
pub struct RemoteMovementBlend {
    endpoint: RemoteMovementPose,
    endpoint_ms: u32,
    duration_ms: u32,
    displacement: Vec3,
    yaw_delta: f32,
    pitch_delta: f32,
    flags: u16,
}

impl RemoteMovementBlend {
    /// Sorted insertion can change the first command without reseeding native
    /// retained deltas. Sampling still reads the current first command's pose.
    pub fn retarget(&mut self, endpoint: RemoteMovementPose, endpoint_ms: u32) {
        self.endpoint = endpoint;
        self.endpoint_ms = endpoint_ms;
    }

    /// Seeds corrections for the next ordinary command with a snapshot.
    /// A teleport or command without a snapshot must not call this boundary.
    #[must_use]
    pub fn new(
        current: RemoteMovementPose,
        endpoint: RemoteMovementPose,
        current_ms: u32,
        endpoint_ms: u32,
        movement_flags: u32,
    ) -> Option<Self> {
        let duration_ms = endpoint_ms.wrapping_sub(current_ms);
        if duration_ms == 0 || current.transport_guid != endpoint.transport_guid {
            return None;
        }
        let mut flags = if movement_flags & 0xc0100f != 0 {
            0x400
        } else {
            0
        };
        if (f64::from(endpoint.transform.orientation())
            - f64::from(current.transform.orientation()))
        .abs()
            >= EPSILON
        {
            flags |= 0x800;
        }
        if movement_flags & 0x2200000 != 0
            && (f64::from(endpoint.pitch) - f64::from(current.pitch)).abs() >= EPSILON
        {
            flags |= 0x1000;
        }
        Some(Self {
            endpoint,
            endpoint_ms,
            duration_ms,
            displacement: endpoint.transform.position() - current.transform.position(),
            yaw_delta: signed_angle(
                endpoint.transform.orientation() - current.transform.orientation(),
            ) as f32,
            pitch_delta: signed_angle(endpoint.pitch - current.pitch) as f32,
            flags,
        })
    }

    /// Blends the analytic endpoint before collision. `start_ms` is the start
    /// of this interval, matching the native frame integrator's call order.
    /// Returns whether position changed and its anchor must be reset.
    pub fn sample(
        &mut self,
        current_position: Vec3,
        start_ms: u32,
        interval_ms: u32,
        analytic: &mut RemoteMovementPose,
    ) -> bool {
        let remaining = self.endpoint_ms.wrapping_sub(start_ms) as i32;
        if remaining < 0 {
            self.flags = 0;
            return false;
        }
        let factor = f64::from(remaining) / f64::from(self.duration_ms);
        let complement = 1.0 - factor;
        if self.flags & 0x400 != 0 {
            let candidate = analytic.transform.position().as_dvec3() * factor
                + (self.endpoint.transform.position().as_dvec3()
                    - self.displacement.as_dvec3() * factor)
                    * complement;
            let delta = candidate.truncate() - current_position.truncate().as_dvec2();
            let maximum = f64::from(interval_ms) * f64::from(f32::from_bits(0x3a83_126f)) * 60.0;
            if delta.length_squared() + EPSILON <= maximum * maximum {
                analytic.transform =
                    WorldTransform::new(candidate.as_vec3(), analytic.transform.orientation());
            } else {
                self.flags &= !0x400;
            }
        }
        if self.flags & 0x800 != 0 {
            let yaw = blend_angle(
                analytic.transform.orientation(),
                self.endpoint.transform.orientation(),
                self.yaw_delta,
                factor,
                complement as f32,
            );
            analytic.transform = WorldTransform::new(analytic.transform.position(), yaw);
        }
        if self.flags & 0x1000 != 0 {
            // After yaw, native reloads the factor from its float spill slot.
            let pitch_factor = if self.flags & 0x800 != 0 {
                f64::from(factor as f32)
            } else {
                factor
            };
            analytic.pitch = blend_angle(
                analytic.pitch,
                self.endpoint.pitch,
                self.pitch_delta,
                pitch_factor,
                complement as f32,
            );
        }
        self.flags & 0x400 != 0
    }
}

fn positive_angle(angle: f32) -> f64 {
    let remainder = f64::from(angle) % std::f64::consts::TAU;
    if remainder < 0.0 {
        remainder + FULL_TURN
    } else {
        remainder
    }
}

fn signed_angle(angle: f32) -> f64 {
    let remainder = f64::from(angle) % std::f64::consts::TAU;
    if remainder < -HALF_TURN {
        remainder + FULL_TURN
    } else if remainder > HALF_TURN {
        remainder - FULL_TURN
    } else {
        remainder
    }
}

fn blend_angle(current: f32, endpoint: f32, delta: f32, factor: f64, complement: f32) -> f32 {
    let target = positive_angle((f64::from(endpoint) - factor * f64::from(delta)) as f32);
    let difference = signed_angle((target - f64::from(current)) as f32);
    positive_angle((difference * f64::from(complement) + f64::from(current)) as f32) as f32
}
