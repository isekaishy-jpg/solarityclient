//! Retained liquid meshes and shared immutable surface sequences on one renderer.

use std::collections::HashMap;
use std::sync::{Arc, Weak};

use glam::{Mat4, Vec3};
use solarity_rendering::{
    BlpColorSpace, BlpTextureHandle, BlpTextureUploadRequest, LiquidDrawMaterial, LiquidFog,
    LiquidLighting, LiquidMeshHandle, LiquidPreparedDraw, LiquidShaderUniform, VulkanRenderer,
    WorldCameraFrame, WorldFrustum, liquid_magma_surface_transform,
};

use crate::application::terrain_frame::RuntimeTerrainFrameError;

use super::{
    ResidentLiquidMaterial, ResidentLiquidShader, ResidentLiquidSurface, ResidentTerrainLiquidBatch,
};

/// One renderer's shared images for an immutable resident material generation.
struct LiquidGpuMaterial {
    source: Arc<ResidentLiquidMaterial>,
    surfaces: Vec<BlpTextureHandle>,
}

/// Weak material generations parallel the CPU cache without retaining departed worlds.
#[derive(Default)]
pub(in crate::application) struct LiquidGpuMaterialCache {
    entries: HashMap<usize, Weak<LiquidGpuMaterial>>,
}

/// One retained terrain batch, with immutable culling bounds and translation.
pub(in crate::application) struct TerrainLiquidGpuBatch {
    material: Arc<LiquidGpuMaterial>,
    mesh: LiquidMeshHandle,
    origin: Vec3,
    minimum: Vec3,
    maximum: Vec3,
}

impl LiquidGpuMaterialCache {
    /// Shares the renderer images for a CPU material's exact selected BLP sources.
    fn prepare(
        &mut self,
        renderer: &mut VulkanRenderer,
        source: &Arc<ResidentLiquidMaterial>,
    ) -> Result<Arc<LiquidGpuMaterial>, RuntimeTerrainFrameError> {
        let key = Arc::as_ptr(source) as usize;
        if let Some(material) = self.entries.get(&key).and_then(Weak::upgrade) {
            return Ok(material);
        }
        let requests = source
            .surfaces
            .iter()
            .filter_map(|surface| match surface {
                ResidentLiquidSurface::Authored(source) => {
                    Some(BlpTextureUploadRequest::new(source, BlpColorSpace::Linear))
                }
                ResidentLiquidSurface::StockFailure => None,
            })
            .collect::<Vec<_>>();
        let uploaded = renderer.upload_blp_textures(&requests)?;
        let mut authored = 0;
        let mut surfaces = Vec::with_capacity(source.surfaces.len());
        for surface in &source.surfaces {
            surfaces.push(match surface {
                ResidentLiquidSurface::Authored(_) => {
                    let handle = uploaded[authored];
                    authored += 1;
                    handle
                }
                // Same Texture.cpp failure image used by ordinary M2 requests.
                ResidentLiquidSurface::StockFailure => renderer.upload_stock_m2_failure()?,
            });
        }
        let material = Arc::new(LiquidGpuMaterial {
            source: Arc::clone(source),
            surfaces,
        });
        self.entries.insert(key, Arc::downgrade(&material));
        Ok(material)
    }

    /// Uploads each admitted strip once; partial uploads retire if a later batch fails.
    pub(in crate::application) fn prepare_terrain(
        &mut self,
        renderer: &mut VulkanRenderer,
        batches: &[ResidentTerrainLiquidBatch],
    ) -> Result<Vec<TerrainLiquidGpuBatch>, RuntimeTerrainFrameError> {
        let mut prepared = Vec::with_capacity(batches.len());
        for batch in batches {
            let next = (|| {
                let material = self.prepare(renderer, &batch.material)?;
                let mesh = renderer.upload_liquid_mesh(&batch.vertices, &batch.indices)?;
                Ok(TerrainLiquidGpuBatch {
                    material,
                    mesh,
                    origin: batch.origin,
                    minimum: batch.minimum,
                    maximum: batch.maximum,
                })
            })();
            match next {
                Ok(batch) => prepared.push(batch),
                Err(error) => {
                    let handles = prepared
                        .iter()
                        .map(TerrainLiquidGpuBatch::mesh)
                        .collect::<Vec<_>>();
                    renderer.retire_liquid_meshes(&handles)?;
                    return Err(error);
                }
            }
        }
        Ok(prepared)
    }
}

impl TerrainLiquidGpuBatch {
    /// Returns the retained handle invalidated when this ADT generation departs.
    pub(in crate::application) const fn mesh(&self) -> LiquidMeshHandle {
        self.mesh
    }

    /// Culls one batch and captures the same camera/light/clock sample as the world frame.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::application) fn prepare_draw(
        &self,
        renderer: &VulkanRenderer,
        frustum: WorldFrustum,
        camera: WorldCameraFrame,
        lighting: LiquidLighting,
        fog: LiquidFog,
        time_ms: u32,
        specular_enabled: bool,
    ) -> Result<Option<LiquidPreparedDraw>, RuntimeTerrainFrameError> {
        let center = (self.minimum + self.maximum) * 0.5;
        let half = (self.maximum - self.minimum) * 0.5;
        if !frustum.intersects_box(center, Vec3::X * half.x, Vec3::Y * half.y, Vec3::Z * half.z)? {
            return Ok(None);
        }
        let (material, surface_transform, depth_transform) = match self.material.source.shader {
            ResidentLiquidShader::Water {
                depth,
                surface,
                depth_scale,
            } => (
                if specular_enabled {
                    LiquidDrawMaterial::Water(depth)
                } else {
                    LiquidDrawMaterial::WaterNoSpecular(depth)
                },
                surface,
                Mat4::from_scale(Vec3::new(1.0, depth_scale, 1.0)),
            ),
            // B23F64 initializes the magma basis scale to one in the pinned image.
            ResidentLiquidShader::Magma { rates } => (
                LiquidDrawMaterial::Magma,
                liquid_magma_surface_transform(rates, 1.0, time_ms)?,
                Mat4::IDENTITY,
            ),
        };
        let frame_index = self.material.source.timeline.frame_index(time_ms) as usize;
        let surface = self.material.surfaces.get(frame_index).copied().ok_or(
            RuntimeTerrainFrameError::LiquidTextureFrame {
                index: frame_index,
                count: self.material.surfaces.len(),
            },
        )?;
        Ok(Some(renderer.prepare_liquid_draw(
            self.mesh,
            material,
            surface,
            LiquidShaderUniform::new(
                camera.projection(),
                camera.view() * Mat4::from_translation(self.origin),
                surface_transform,
                depth_transform,
                lighting,
                fog,
            ),
        )?))
    }
}
