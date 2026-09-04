//! Raster and transform orientation for stock attached M2 models.

use glam::Mat4;

/// Selects ordinary or reflected M2 geometry and its corresponding winding.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum M2ModelOrientation {
    /// Preserve authored local coordinates and counter-clockwise winding.
    #[default]
    Authored,
    /// Reflect local X and treat clockwise triangles as front-facing.
    Mirrored,
}

impl M2ModelOrientation {
    /// Returns the local transform paired with this raster orientation.
    #[must_use]
    pub fn local_transform(self) -> Mat4 {
        match self {
            Self::Authored => Mat4::IDENTITY,
            Self::Mirrored => Mat4::from_scale(glam::Vec3::new(-1.0, 1.0, 1.0)),
        }
    }
}
