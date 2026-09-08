//! Retained native cloud inputs for one world submission.

use glam::{Mat4, Vec3, Vec4};

use super::{WorldCloudDome, WorldClouds};

/// One visible procedural texture bank and its camera-relative dome.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldCloudFrame<'a> {
    dome: &'a WorldCloudDome,
    pixels: &'a [u8],
    view_projection: Mat4,
}

impl<'a> WorldCloudFrame<'a> {
    /// Retains the native mesh and current texture at the final world camera.
    #[must_use]
    pub fn new(
        dome: &'a WorldCloudDome,
        clouds: &'a WorldClouds,
        camera: crate::WorldCameraFrame,
    ) -> Self {
        let mut rotation = camera.view();
        rotation.w_axis = Vec4::W;
        Self {
            dome,
            pixels: clouds.bgra8(),
            view_projection: camera.projection()
                * rotation
                * Mat4::from_scale(Vec3::splat(6.666_666_5)),
        }
    }

    /// Returns the fixed native dome.
    #[must_use]
    pub const fn dome(self) -> &'a WorldCloudDome {
        self.dome
    }

    /// Returns the simulation's visible texture bank.
    #[must_use]
    pub const fn bgra8(self) -> &'a [u8] {
        self.pixels
    }

    /// Returns the native sky-scale camera transform.
    #[must_use]
    pub const fn view_projection(self) -> Mat4 {
        self.view_projection
    }
}
