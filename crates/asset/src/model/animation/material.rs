//! Build-12340 animated M2 material properties.

use glam::{Quat, Vec3};

use super::M2Track;

/// Animated RGB and alpha selected by a SKIN batch color index.
#[derive(Clone, Debug, PartialEq)]
pub struct M2ColorAnimation {
    color: M2Track<Vec3>,
    alpha: M2Track<f32>,
}

impl M2ColorAnimation {
    pub(super) const fn new(color: M2Track<Vec3>, alpha: M2Track<f32>) -> Self {
        Self { color, alpha }
    }

    /// Returns the authored RGB track.
    #[must_use]
    pub const fn color(&self) -> &M2Track<Vec3> {
        &self.color
    }

    /// Returns signed fixed16 alpha normalized to the stock floating domain.
    #[must_use]
    pub const fn alpha(&self) -> &M2Track<f32> {
        &self.alpha
    }
}

/// Animated texture-weight multiplier selected through the lookup table.
#[derive(Clone, Debug, PartialEq)]
pub struct M2TextureWeight {
    weight: M2Track<f32>,
}

impl M2TextureWeight {
    pub(super) const fn new(weight: M2Track<f32>) -> Self {
        Self { weight }
    }

    /// Returns signed fixed16 opacity normalized by `32767`.
    #[must_use]
    pub const fn weight(&self) -> &M2Track<f32> {
        &self.weight
    }
}

/// Animated texture translation, compressed rotation, and scale tracks.
#[derive(Clone, Debug, PartialEq)]
pub struct M2TextureTransform {
    translation: M2Track<Vec3>,
    rotation: M2Track<Quat>,
    scale: M2Track<Vec3>,
}

impl M2TextureTransform {
    pub(super) const fn new(
        translation: M2Track<Vec3>,
        rotation: M2Track<Quat>,
        scale: M2Track<Vec3>,
    ) -> Self {
        Self {
            translation,
            rotation,
            scale,
        }
    }

    /// Returns the authored UV-space translation track.
    #[must_use]
    pub const fn translation(&self) -> &M2Track<Vec3> {
        &self.translation
    }

    /// Returns the authored compressed-quaternion rotation track.
    #[must_use]
    pub const fn rotation(&self) -> &M2Track<Quat> {
        &self.rotation
    }

    /// Returns the authored UV-space scale track.
    #[must_use]
    pub const fn scale(&self) -> &M2Track<Vec3> {
        &self.scale
    }
}
