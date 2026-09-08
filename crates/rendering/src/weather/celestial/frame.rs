//! Retained celestial color state and camera-relative frame transforms.

use super::{WorldCelestialBody, WorldCelestialMesh};
use crate::{BlpTextureHandle, WorldCameraFrame};
use glam::{Mat4, Vec3, Vec4};

/// Native 9D0760 colors, updated by the celestial portion of 7F3230.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WorldCelestialLighting {
    colors: [u32; 3],
}

impl WorldCelestialLighting {
    /// Applies band nine and the global weather attenuation. The second moon
    /// retains its constructor RGB/alpha except for nonzero weather updates.
    pub fn update(&mut self, color: Vec3, weather: f32) {
        let [r, g, b] = color
            .to_array()
            .map(|c| (c * 255.).round_ties_even() as u32);
        let packed = 0xff00_0000 | r << 16 | g << 8 | b;
        self.colors[0] = packed;
        self.colors[1] = packed;
        if weather != 0. {
            let alpha = ((1. - f64::from(weather)) * 255.) as f32;
            let alpha = (alpha.round_ties_even() as u32 & 255) << 24;
            for color in &mut self.colors {
                *color = (*color & 0x00ff_ffff) | alpha;
            }
        }
    }

    /// Returns sun, first moon and second moon packed ARGB colors.
    #[must_use]
    pub const fn colors(self) -> [u32; 3] {
        self.colors
    }
}

/// One retained celestial texture and native clipped strip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldCelestialDraw<'a> {
    mesh: &'a WorldCelestialMesh,
    texture: BlpTextureHandle,
    view_projection: Mat4,
}

impl<'a> WorldCelestialDraw<'a> {
    /// Combines 9ABB60's native positive-forward basis with 9AC660's body offset.
    #[must_use]
    pub fn new(
        mesh: &'a WorldCelestialMesh,
        body: WorldCelestialBody,
        texture: BlpTextureHandle,
        camera: WorldCameraFrame,
    ) -> Self {
        let mut model = super::mesh::basis(camera.forward());
        model.w_axis = (body.position() - camera.camera().position()).extend(1.);
        let mut view = camera.view();
        view.w_axis = Vec4::W;
        Self {
            mesh,
            texture,
            view_projection: camera.projection() * view * model,
        }
    }
    /// Returns the complete six-vertex storage and admitted strip.
    #[must_use]
    pub const fn mesh(self) -> &'a WorldCelestialMesh {
        self.mesh
    }
    /// Returns the resident authored BLP texture.
    #[must_use]
    pub const fn texture(self) -> BlpTextureHandle {
        self.texture
    }
    /// Returns the body-to-clip transform, before the native sky depth remap.
    #[must_use]
    pub const fn view_projection(self) -> Mat4 {
        self.view_projection
    }
}

/// Sun and both moons in 7F09B0's order, before the additive sky gradient.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldCelestialFrame<'a> {
    draws: [WorldCelestialDraw<'a>; 3],
}

impl<'a> WorldCelestialFrame<'a> {
    /// Retains all three native slots; the below-horizon slots have empty strips.
    #[must_use]
    pub const fn new(draws: [WorldCelestialDraw<'a>; 3]) -> Self {
        Self { draws }
    }
    /// Returns the draws in native order.
    #[must_use]
    pub const fn draws(self) -> [WorldCelestialDraw<'a>; 3] {
        self.draws
    }
    /// Returns the number of admitted body strips.
    #[must_use]
    pub fn draw_count(self) -> usize {
        self.draws
            .iter()
            .filter(|draw| !draw.mesh.indices().is_empty())
            .count()
    }
}
