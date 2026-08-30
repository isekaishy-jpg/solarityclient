//! RTTI-backed type, state, and identifier vocabulary for this stock responsibility.

use thiserror::Error;

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
