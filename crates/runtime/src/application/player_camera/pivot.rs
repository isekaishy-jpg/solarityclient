//! Camera.cpp's stationary-eye pitch channel (`6020B0`, `5FFDE0`, `6012D0`).

use super::PlayerCameraMouseSettings;
use super::follow::FollowAngle;

/// Registered pivot admission thresholds and final-view return speed.
#[derive(Clone, Copy)]
pub(in crate::application) struct PivotSettings {
    /// Enables ground-contact admission; an existing offset retains its return lane.
    pub enabled: bool,
    /// Strict upper bound on absolute mouse yaw in radians per input event.
    pub maximum_yaw_delta: f32,
    /// Strict lower bound on absolute raw mouse pitch in radians per input event.
    pub minimum_pitch_delta: f32,
    /// Independent target-angle return speed in degrees per second.
    pub return_speed: f32,
}

impl Default for PivotSettings {
    fn default() -> Self {
        // 5FD910's C249AC / C249A8 / C249A4 / C24E34 registrations.
        Self {
            enabled: true,
            maximum_yaw_delta: 0.05,
            minimum_pitch_delta: 0.0,
            return_speed: 90.0,
        }
    }
}

impl PivotSettings {
    /// Reads the camera's live CVar snapshot, including the raw-radian thresholds.
    pub fn read(number: &impl Fn(&str) -> Option<f32>) -> Self {
        Self {
            enabled: number("cameraPivot").unwrap_or(1.0) != 0.0,
            maximum_yaw_delta: number("cameraPivotDXMax").unwrap_or(0.05),
            minimum_pitch_delta: number("cameraPivotDYMin").unwrap_or(0.0),
            return_speed: number("cameraTargetSmoothSpeed").unwrap_or(90.0),
        }
    }
}

/// The ordinary unit branch of 5FFDE0 distinguishes anchor and orbit contacts.
pub(super) fn admitted(orbit: f32, flags: u32, movement: u32, settings: PivotSettings) -> bool {
    settings.enabled
        && movement & 0xf == 0
        && orbit <= 0.0
        && flags & 8 == 0
        && flags
            & if movement & 0x0200_0000 != 0 {
                0x10000
            } else {
                0x30000
            }
            != 0
}

/// Applies a complete mouse event while keeping the pivot offset out of orbit geometry.
pub(super) fn motion(
    offset: &mut FollowAngle,
    orbit: &mut f32,
    flags: u32,
    movement: u32,
    angles: [f32; 2],
    settings: PlayerCameraMouseSettings,
    time: u32,
) {
    let [yaw, pitch] = angles;
    let sign = if settings.invert_pitch { -1.0 } else { 1.0 };
    // The positive-input reversal test is before inversion in stock.
    let mut raw_pitch = pitch * sign;
    let mut pivoting = !offset.active && offset.current.abs() >= 0.001_f32;
    if admitted(*orbit, flags, movement, settings.pivot) {
        if raw_pitch.abs() > settings.pivot.minimum_pitch_delta
            && yaw.abs() < settings.pivot.maximum_yaw_delta
        {
            pivoting = true;
        }
        if raw_pitch > 0.0 {
            let crosses_zero = offset.current < 0.0
                && f64::from(sign) * f64::from(raw_pitch) + f64::from(offset.current) > 0.0;
            if crosses_zero || offset.current.abs() < f32::EPSILON * 2.0 {
                if crosses_zero {
                    raw_pitch += offset.current;
                }
                offset.current = 0.0;
                offset.set_goal(0.0);
                pivoting = false;
            }
        }
    }
    let mut orbiting = !pivoting;
    if pivoting {
        offset.current =
            (f64::from(sign) * f64::from(raw_pitch) + f64::from(offset.current)) as f32;
        orbiting |= *orbit >= 0.0;
        let minimum_offset = f64::from(-1.553_343_f32) - f64::from(*orbit);
        if *orbit < 0.0 && f64::from(offset.current) < minimum_offset {
            offset.current = minimum_offset as f32;
        }
    }
    if offset.current > 0.0 || orbiting {
        offset.request(0.0, 0.0, 1.0, settings.pivot.return_speed, time);
        *orbit = (*orbit + sign * raw_pitch).clamp(-1.553_343, 1.553_343);
    } else {
        offset.cancel();
    }
}
