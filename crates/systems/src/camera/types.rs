//! RTTI-backed type, state, and identifier vocabulary for this stock responsibility.

use glam::Vec3;
use thiserror::Error;

/// Resolved third-person view vectors and the two distinct followed positions.
///
/// Build 12340 passes the eye, one-unit target, collision pivot, and followed
/// object position to different consumers. Keeping all four prevents later
/// residency and collision code from treating the character as the look-at
/// target or the elevated orbit pivot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerCameraPose {
    eye: Vec3,
    target: Vec3,
    up: Vec3,
    orbit_pivot: Vec3,
    subject: Vec3,
}

impl PlayerCameraPose {
    pub(super) const fn new(
        eye: Vec3,
        target: Vec3,
        up: Vec3,
        orbit_pivot: Vec3,
        subject: Vec3,
    ) -> Self {
        Self {
            eye,
            target,
            up,
            orbit_pivot,
            subject,
        }
    }

    /// Returns the world-space camera position before obstruction correction.
    #[must_use]
    pub const fn eye(self) -> Vec3 {
        self.eye
    }

    /// Returns the one-unit look direction endpoint used by projection.
    #[must_use]
    pub const fn target(self) -> Vec3 {
        self.target
    }

    /// Returns the continuous orbit roll reference.
    #[must_use]
    pub const fn up(self) -> Vec3 {
        self.up
    }

    /// Returns the character-height-adjusted collision pivot.
    #[must_use]
    pub const fn orbit_pivot(self) -> Vec3 {
        self.orbit_pivot
    }

    /// Returns the followed object's authoritative world position.
    #[must_use]
    pub const fn subject(self) -> Vec3 {
        self.subject
    }
}

/// Invalid runtime state at the player-orbit boundary.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PlayerCameraPoseError {
    /// The authoritative position or orientation is not finite.
    #[error("player camera transform is not finite")]
    NonFiniteTransform,
    /// A saved camera distance, pitch, or yaw is not finite.
    #[error("player camera view state is not finite")]
    NonFiniteView,
    /// The authored camera subject height is not finite.
    #[error("player camera subject height is not finite")]
    NonFiniteSubjectHeight,
}

/// Authored M2 inputs consumed by build 12340's unit camera-height path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraSubjectGeometry {
    breath_attachment_z: Option<f32>,
    model_sphere_radius: f32,
    object_scale: f32,
}

impl CameraSubjectGeometry {
    /// Captures the stable authored Breath height, model sphere, and server scale.
    #[must_use]
    pub const fn new(
        breath_attachment_z: Option<f32>,
        model_sphere_radius: f32,
        object_scale: f32,
    ) -> Self {
        Self {
            breath_attachment_z,
            model_sphere_radius,
            object_scale,
        }
    }

    pub(super) const fn breath_attachment_z(self) -> Option<f32> {
        self.breath_attachment_z
    }

    pub(super) const fn model_sphere_radius(self) -> f32 {
        self.model_sphere_radius
    }

    pub(super) const fn object_scale(self) -> f32 {
        self.object_scale
    }
}

/// Authored branch selected for a resolved camera pivot height.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CameraSubjectHeightSource {
    /// Character M2 attachment ID 17 supplied the height.
    BreathAttachment,
    /// The stock non-attachment branch used 99% of the model sphere.
    ModelSphere,
}

/// Validated local-player camera pivot height and its authored source.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraSubjectHeight {
    value: f32,
    source: CameraSubjectHeightSource,
}

impl CameraSubjectHeight {
    pub(super) const fn new(value: f32, source: CameraSubjectHeightSource) -> Self {
        Self { value, source }
    }

    /// Returns the clamped world-space pivot height above the unit origin.
    #[must_use]
    pub const fn value(self) -> f32 {
        self.value
    }

    /// Returns which authored stock branch supplied the height.
    #[must_use]
    pub const fn source(self) -> CameraSubjectHeightSource {
        self.source
    }
}

/// Invalid authored data at the unit-to-camera boundary.
#[derive(Clone, Copy, Debug, Error, PartialEq)]
pub enum CameraSubjectHeightError {
    /// `OBJECT_FIELD_SCALE_X` is not finite.
    #[error("camera subject object scale is not finite")]
    NonFiniteScale,
    /// The authored Breath attachment height is not finite.
    #[error("camera subject Breath attachment height is not finite")]
    NonFiniteBreathAttachment,
    /// The fallback model sphere is negative or not finite.
    #[error("camera subject model sphere radius is invalid")]
    InvalidModelSphere,
}
