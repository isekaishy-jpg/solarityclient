//! Native primary camera anchor, center ray and swept volume (605D60).

use glam::Vec3;
use thiserror::Error;

use super::types::PlayerCameraOrbit;

use super::{
    PlayerCameraLiquidState, PlayerCameraPose, PlayerCameraVolume, PlayerCameraVolumeKind,
    PlayerCameraVolumeQueryError, resolve_player_camera_volume,
};

const MINIMUM_HEIGHT: f32 = 0.833_333_3;
const RETREAT: f32 = 0.111_111_11;
const EPSILON: f32 = 0.000_000_953_674_3;

/// Registered subject state supplied to the native camera constraints.
#[derive(Clone, Copy, Debug)]
pub struct PlayerCameraObstructionSettings {
    /// Live `cameraWaterCollision`, controlling primary water geometry.
    pub water_collision: bool,
    /// Classification from the subject registration, before camera elevation.
    pub subject_liquid: PlayerCameraLiquidState,
    /// Unit-specific minimum camera anchor, normally 75% of unit height.
    pub minimum_subject_height: Option<f32>,
}

impl Default for PlayerCameraObstructionSettings {
    fn default() -> Self {
        Self {
            water_collision: true,
            subject_liquid: PlayerCameraLiquidState::Absent,
            minimum_subject_height: None,
        }
    }
}

/// A camera scene operation in native call order.
pub enum PlayerCameraSceneQuery<'a> {
    /// 77F310 center or vertical-anchor trace, with solid geometry and optional water.
    Segment {
        /// Trace origin.
        start: Vec3,
        /// Trace endpoint.
        end: Vec3,
        /// Whether the native mask includes water bit 0x20000.
        water: bool,
    },
    /// 77F330 triangle-volume collection, returning the greatest cube-Z retreat.
    Volume {
        /// The oriented camera volume.
        volume: &'a PlayerCameraVolume,
        /// Native geometry bank.
        kind: PlayerCameraVolumeKind,
    },
}

/// Primary obstruction result, before 6061D0's final water-interface correction.
pub struct PlayerCameraObstruction {
    pose: PlayerCameraPose,
    distance: f32,
    height: f32,
    contacts: PlayerCameraContacts,
}

/// Native primary contact bits retained for the next mouse-pivot decision.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PlayerCameraContacts {
    /// The vertical subject-anchor ray hit geometry (`0x20000`).
    pub anchor: bool,
    /// The orbit center ray or swept volume hit geometry (`0x10000`).
    pub orbit: bool,
}

impl PlayerCameraObstruction {
    /// Returns actual hit results, including hits that do not shorten distance.
    pub const fn contacts(&self) -> PlayerCameraContacts {
        self.contacts
    }
    /// Returns the local anchor height without subtracting rounded world Z.
    #[must_use]
    pub const fn height(&self) -> f32 {
        self.height
    }
    /// Returns the primary eye, anchor and retained view orientation.
    #[must_use]
    pub const fn pose(&self) -> PlayerCameraPose {
        self.pose
    }

    /// Returns the resolved zoom distance for the persistent recovery lane.
    #[must_use]
    pub const fn distance(&self) -> f32 {
        self.distance
    }
}

/// Invalid primary camera inputs or rejected native scene geometry.
#[derive(Debug, Error)]
pub enum PlayerCameraObstructionError<E> {
    /// Camera vectors or subject constraints cannot form a finite basis.
    #[error("camera obstruction basis is invalid")]
    InvalidCameraBasis,
    /// A provider returned a fraction outside its requested segment interval.
    #[error("camera obstruction trace returned an invalid fraction")]
    InvalidTraceFraction,
    /// A scene owner rejected a ray.
    #[error("camera obstruction trace failed")]
    Trace(#[source] E),
    /// Volume construction or its geometry provider failed.
    #[error(transparent)]
    Volume(#[from] PlayerCameraVolumeQueryError<E>),
}

/// Resolves 605D60's vertical anchor, center ray and two swept geometry banks.
/// The one-ninth retreat follows both ray and volume results. View orientation
/// is retained; the returned distance feeds zoom recovery before final water
/// interface correction.
///
/// # Errors
/// Rejects malformed input, invalid provider fractions or scene failures.
pub fn resolve_player_camera_obstruction<E>(
    pose: PlayerCameraPose,
    aspect_ratio: f32,
    settings: PlayerCameraObstructionSettings,
    mut scene: impl FnMut(PlayerCameraSceneQuery<'_>) -> Result<Option<f32>, E>,
) -> Result<PlayerCameraObstruction, PlayerCameraObstructionError<E>> {
    // 605D60 reads the camera's distance/height banks and virtual forward
    // vector directly. Reconstructing them from quantized world positions
    // can toggle the one-ninth retreat across successive stationary frames.
    let orbit = pose.orbit();
    let forward = orbit.forward;
    let input = PrimaryInput {
        subject: pose.subject(),
        forward,
        up: pose.up(),
        distance: orbit.distance,
        height: orbit.height,
        mount_height: pose.flying_mount_height(),
    };
    let resolved = resolve_primary(input, aspect_ratio, settings, &mut scene)?;
    let pivot = Vec3::new(
        input.subject.x,
        input.subject.y,
        input.subject.z + resolved.height,
    );
    let eye = camera_eye(input, pivot, resolved.distance, resolved.vertical_fraction);
    Ok(PlayerCameraObstruction {
        pose: PlayerCameraPose::new(
            eye,
            eye + forward,
            pose.up(),
            pivot,
            pose.subject(),
            pose.flying_mount_height(),
            PlayerCameraOrbit {
                distance: resolved.distance,
                height: resolved.height,
                ..orbit
            },
        ),
        distance: resolved.distance,
        height: resolved.height,
        contacts: resolved.contacts,
    })
}

#[derive(Clone, Copy)]
/// Native primary inputs retain scalar banks independently of world coordinates.
struct PrimaryInput {
    subject: Vec3,
    forward: Vec3,
    up: Vec3,
    distance: f32,
    height: f32,
    mount_height: f32,
}
/// Primary scalar results before composing the final eye.
struct PrimaryResult {
    distance: f32,
    height: f32,
    vertical_fraction: f32,
    contacts: PlayerCameraContacts,
}

/// Applies native anchor limits, center tracing and swept-volume retreat in order.
fn resolve_primary<E>(
    input: PrimaryInput,
    aspect: f32,
    settings: PlayerCameraObstructionSettings,
    scene: &mut impl FnMut(PlayerCameraSceneQuery<'_>) -> Result<Option<f32>, E>,
) -> Result<PrimaryResult, PlayerCameraObstructionError<E>> {
    let depth = match settings.subject_liquid {
        PlayerCameraLiquidState::Absent => 0.0,
        PlayerCameraLiquidState::Surface { depth }
        | PlayerCameraLiquidState::Submerged { depth } => depth,
    };
    if !input.subject.is_finite()
        || !input.forward.is_finite()
        || !input.up.is_finite()
        || ![input.distance, input.height, input.mount_height, depth]
            .into_iter()
            .all(f32::is_finite)
        || settings
            .minimum_subject_height
            .is_some_and(|height| !height.is_finite())
    {
        return Err(PlayerCameraObstructionError::InvalidCameraBasis);
    }
    let mut height = input.height;
    let mut lower = MINIMUM_HEIGHT;
    let mut upper = input.height;
    let mut vertical_fraction = 1.0;
    let mut contacts = PlayerCameraContacts::default();
    if f64::from(height) - f64::from(0.2_f32) > f64::from(EPSILON) {
        if settings.water_collision {
            match settings.subject_liquid {
                PlayerCameraLiquidState::Surface { depth } => {
                    lower = depth + super::PLAYER_CAMERA_WATER_CLEARANCE;
                    upper = upper.max(lower);
                }
                PlayerCameraLiquidState::Submerged { depth } => {
                    upper = (depth - MINIMUM_HEIGHT).max(MINIMUM_HEIGHT);
                }
                PlayerCameraLiquidState::Absent => {}
            }
        }
        let base_z = input.subject.z + lower;
        let span = ((height - lower) + input.mount_height).max(RETREAT);
        height = span;
        if let Some(fraction) = trace(
            scene,
            Vec3::new(input.subject.x, input.subject.y, base_z),
            Vec3::new(input.subject.x, input.subject.y, base_z + span),
            settings.water_collision,
        )? {
            contacts.anchor = true;
            vertical_fraction = fraction;
            height *= fraction;
        }
        if (f64::from(span) - f64::from(height)).abs() >= f64::from(f32::EPSILON * 2.0) {
            height = (height - RETREAT).max(RETREAT);
        }
        height = (height + lower) - input.mount_height;
        if let Some(minimum) = settings.minimum_subject_height {
            height = height.max(minimum);
        }
    }
    height = height.max(lower).min(upper);
    let pivot = Vec3::new(input.subject.x, input.subject.y, input.subject.z + height);
    let mut distance = input.distance;
    if f64::from(distance) - f64::from(0.2_f32) > f64::from(EPSILON) {
        let desired = (pivot.as_dvec3() - input.forward.as_dvec3() * f64::from(distance)
            + input.up.as_dvec3() * f64::from(input.mount_height) * f64::from(vertical_fraction))
        .as_vec3();
        if let Some(fraction) = trace(scene, pivot, desired, settings.water_collision)? {
            contacts.orbit = true;
            distance *= fraction;
        }
        if distance > EPSILON {
            let eye = camera_eye(input, pivot, distance, vertical_fraction);
            if let Some(shortened) = resolve_player_camera_volume(
                distance,
                eye,
                pivot,
                aspect,
                settings.water_collision,
                |volume, kind| scene(PlayerCameraSceneQuery::Volume { volume, kind }),
            )? {
                contacts.orbit = true;
                distance = shortened;
            }
            if (f64::from(input.distance) - f64::from(distance)).abs()
                >= f64::from(f32::EPSILON * 2.0)
            {
                distance = (distance - RETREAT).max(0.0);
            }
        }
    }
    Ok(PrimaryResult {
        distance: distance.min(input.distance),
        height,
        vertical_fraction,
        contacts,
    })
}

/// Composes 601D60's eye with the distance-attenuated flying-mount offset.
fn camera_eye(input: PrimaryInput, pivot: Vec3, distance: f32, vertical_fraction: f32) -> Vec3 {
    let mut eye = (pivot.as_dvec3() - input.forward.as_dvec3() * f64::from(distance)).as_vec3();
    if distance > 0.0 && input.mount_height.abs() >= f32::EPSILON * 2.0 && input.distance > 0.0 {
        let offset =
            (f64::from(vertical_fraction) * f64::from(input.mount_height) * f64::from(distance)
                / f64::from(input.distance)) as f32;
        eye = (eye.as_dvec3() + input.up.as_dvec3() * f64::from(offset)).as_vec3();
    }
    eye
}

/// Validates a scene provider's optional segment fraction without substituting geometry.
fn trace<E>(
    scene: &mut impl FnMut(PlayerCameraSceneQuery<'_>) -> Result<Option<f32>, E>,
    start: Vec3,
    end: Vec3,
    water: bool,
) -> Result<Option<f32>, PlayerCameraObstructionError<E>> {
    let result = scene(PlayerCameraSceneQuery::Segment { start, end, water })
        .map_err(PlayerCameraObstructionError::Trace)?;
    if result.is_some_and(|fraction| !fraction.is_finite() || !(0.0..=1.0).contains(&fraction)) {
        return Err(PlayerCameraObstructionError::InvalidTraceFraction);
    }
    Ok(result)
}
