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
    flying_mount_height: f32,
}

impl PlayerCameraPose {
    pub(super) const fn new(
        eye: Vec3,
        target: Vec3,
        up: Vec3,
        orbit_pivot: Vec3,
        subject: Vec3,
        flying_mount_height: f32,
    ) -> Self {
        Self {
            eye,
            target,
            up,
            orbit_pivot,
            subject,
            flying_mount_height,
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

    /// Returns the separately smoothed `$CFM` collision-height input.
    ///
    /// This is not part of [`Self::orbit_pivot`]. Build 12340 consumes it
    /// while constructing the camera obstruction traces.
    #[must_use]
    pub const fn flying_mount_height(self) -> f32 {
        self.flying_mount_height
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
    /// The separately smoothed `$CFM` collision height is not finite.
    #[error("player camera flying-mount height is not finite")]
    NonFiniteFlyingMountHeight,
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
    /// A live `$CMA` event on the mounted model supplied the principal height.
    AnimatedMountMarker,
}

/// One sampled pair of build-12340 player-camera height inputs.
///
/// The principal height forms the orbit pivot. The `$CFM` value remains a
/// distinct obstruction input and must never be added to that pivot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerCameraHeightSample {
    subject_height: CameraSubjectHeight,
    flying_mount_height: f32,
}

impl PlayerCameraHeightSample {
    pub(super) const fn new(subject_height: CameraSubjectHeight, flying_mount_height: f32) -> Self {
        Self {
            subject_height,
            flying_mount_height,
        }
    }

    /// Returns the principal camera-pivot height.
    #[must_use]
    pub const fn subject_height(self) -> CameraSubjectHeight {
        self.subject_height
    }

    /// Returns the separately smoothed `$CFM` collision-height input.
    #[must_use]
    pub const fn flying_mount_height(self) -> f32 {
        self.flying_mount_height
    }
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

/// The two mutually exclusive mount-camera declarations read by build 12340.
///
/// `$CMA` is evaluated through the current mount bone pose and expressed as a
/// height above that model's transformed origin. `$CFM` retains its authored
/// local Z value and is latched once for the current mount generation.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MountCameraGeometry {
    animated_height: Option<f32>,
    fixed_height: Option<f32>,
}

impl MountCameraGeometry {
    /// Captures the current `$CMA` height and authored `$CFM` height.
    #[must_use]
    pub const fn new(animated_height: Option<f32>, fixed_height: Option<f32>) -> Self {
        Self {
            animated_height,
            fixed_height,
        }
    }

    pub(super) const fn animated_height(self) -> Option<f32> {
        self.animated_height
    }

    pub(super) const fn fixed_height(self) -> Option<f32> {
        self.fixed_height
    }
}

/// Invalid time or marker geometry at the mounted-camera boundary.
#[derive(Clone, Copy, Debug, Error, PartialEq)]
pub enum MountCameraHeightError {
    /// The caller supplied a non-finite process time.
    #[error("mount camera time is not finite")]
    NonFiniteTime,
    /// The transformed `$CMA` height is not finite.
    #[error("animated mount camera height is not finite")]
    NonFiniteAnimatedHeight,
    /// The authored `$CFM` height is not finite.
    #[error("fixed mount camera height is not finite")]
    NonFiniteFixedHeight,
}
