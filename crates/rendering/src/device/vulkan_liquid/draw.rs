//! Complete liquid draw packets and one coherent frame's procedural depth images.

use crate::device::BlpTextureHandle;
use crate::{LiquidDepthTexture, LiquidDepthTextureKind, LiquidShader, LiquidShaderUniform};

use super::LiquidMeshHandle;

/// Couples each native shader family to the depth input it actually consumes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiquidDrawMaterial {
    /// Ordinary water with its directional specular contribution.
    Water(LiquidDepthTextureKind),
    /// Water selected when the native specular setting is disabled.
    WaterNoSpecular(LiquidDepthTextureKind),
    /// Opaque magma/slime consumes only its animated surface texture.
    Magma,
}

impl LiquidDrawMaterial {
    /// Selects the closed native programmable shader family.
    pub(in crate::device) const fn shader(self) -> LiquidShader {
        match self {
            Self::Water(_) => LiquidShader::Water,
            Self::WaterNoSpecular(_) => LiquidShader::WaterNoSpecular,
            Self::Magma => LiquidShader::Magma,
        }
    }

    /// Returns the stock depth texture slot when the shader reads one.
    pub(in crate::device) const fn depth(self) -> Option<LiquidDepthTextureKind> {
        match self {
            Self::Water(kind) | Self::WaterNoSpecular(kind) => Some(kind),
            Self::Magma => None,
        }
    }
}

/// A retained strip, resident animated frame, and exact per-draw lighting sample.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiquidPreparedDraw {
    mesh: LiquidMeshHandle,
    material: LiquidDrawMaterial,
    surface: BlpTextureHandle,
    uniform: LiquidShaderUniform,
}

impl LiquidPreparedDraw {
    /// Publishes the packet only after the renderer validates its resource handles.
    pub(in crate::device) const fn new(
        mesh: LiquidMeshHandle,
        material: LiquidDrawMaterial,
        surface: BlpTextureHandle,
        uniform: LiquidShaderUniform,
    ) -> Self {
        Self {
            mesh,
            material,
            surface,
            uniform,
        }
    }

    pub(in crate::device) const fn mesh(self) -> LiquidMeshHandle {
        self.mesh
    }
    pub(in crate::device) const fn material(self) -> LiquidDrawMaterial {
        self.material
    }
    pub(in crate::device) const fn surface(self) -> BlpTextureHandle {
        self.surface
    }
    pub(in crate::device) const fn uniform(self) -> LiquidShaderUniform {
        self.uniform
    }
}

/// Borrows all liquid draws and the three stock environment images for one frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiquidFrame<'a> {
    draws: &'a [LiquidPreparedDraw],
    depths: [&'a LiquidDepthTexture; 3],
    water_scene_order: u32,
}

impl<'a> LiquidFrame<'a> {
    /// Joins prepared draws with river, ocean, and WMO depth callbacks at one time.
    ///
    /// `water_scene_order` inserts the transparent liquid queue before that M2
    /// scene ordinal. Native 4F8EA0 places water between transparent passes,
    /// changing which pass precedes it when the camera crosses the surface.
    #[must_use]
    pub const fn new(
        draws: &'a [LiquidPreparedDraw],
        river: &'a LiquidDepthTexture,
        ocean: &'a LiquidDepthTexture,
        world_model: &'a LiquidDepthTexture,
        water_scene_order: u32,
    ) -> Self {
        Self {
            draws,
            depths: [river, ocean, world_model],
            water_scene_order,
        }
    }

    pub(in crate::device) const fn draws(self) -> &'a [LiquidPreparedDraw] {
        self.draws
    }
    pub(in crate::device) const fn depths(self) -> [&'a LiquidDepthTexture; 3] {
        self.depths
    }
    pub(in crate::device) const fn water_scene_order(self) -> u32 {
        self.water_scene_order
    }
}

/// Preserves the fixed procedural texture order independently of enum layout.
pub(super) const fn depth_index(kind: LiquidDepthTextureKind) -> usize {
    match kind {
        LiquidDepthTextureKind::River => 0,
        LiquidDepthTextureKind::Ocean => 1,
        LiquidDepthTextureKind::WorldModel => 2,
    }
}
