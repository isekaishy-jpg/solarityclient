//! Three original 7D0050 terrain point-light register groups.

use glam::Vec3;

/// One terrain point light relative to the world camera, before view rotation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainPointLight {
    position: Vec3,
    diffuse: Vec3,
    attenuation: Vec3,
}

impl TerrainPointLight {
    /// Retains the raw native diffuse color and distance-polynomial coefficients.
    #[must_use]
    pub const fn new(position: Vec3, diffuse: Vec3, attenuation: Vec3) -> Self {
        Self {
            position,
            diffuse,
            attenuation,
        }
    }

    /// Returns an unused native light register group.
    #[must_use]
    pub const fn disabled() -> Self {
        Self::new(Vec3::ZERO, Vec3::ZERO, Vec3::X)
    }

    /// Returns the source position minus the current world camera position.
    #[must_use]
    pub const fn position(self) -> Vec3 {
        self.position
    }

    /// Returns the source's raw animated diffuse color, without byte conversion.
    #[must_use]
    pub const fn diffuse(self) -> Vec3 {
        self.diffuse
    }

    /// Returns constant, linear and quadratic attenuation coefficients.
    #[must_use]
    pub const fn attenuation(self) -> Vec3 {
        self.attenuation
    }

    pub(crate) fn words(self) -> [u32; 9] {
        [
            self.position.x,
            self.position.y,
            self.position.z,
            self.diffuse.x,
            self.diffuse.y,
            self.diffuse.z,
            self.attenuation.x,
            self.attenuation.y,
            self.attenuation.z,
        ]
        .map(f32::to_bits)
    }
}
