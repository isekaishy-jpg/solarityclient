//! Renderer-local WMO resources and allocation-reusing MODF visibility.

use std::collections::HashMap;
use std::sync::Arc;

use glam::Vec3;
use solarity_rendering::{
    BlpColorSpace, BlpTextureHandle, BlpTextureUploadRequest, PlacedWorldModelDrawPlan,
    VulkanError, VulkanRenderer, WorldFrustum, WorldModelBaseMip, WorldModelMaterialState,
    WorldModelMeshHandle, WorldModelMeshPlan, WorldModelPipelineHandle, WorldModelPreparedDraw,
    WorldModelSampledTexture, WorldModelSurfacePassPlan, WorldModelTextureFiltering,
    WorldModelTextureSet, WorldModelTextureSetHandle,
};

use crate::application::terrain_coordinator::world_model_residency::{
    ResidentWorldModelMaterialTextures, ResidentWorldModelScene, ResidentWorldModelTexture,
};

use super::RuntimeTerrainFrameError;

/// One physical pipeline and descriptor pair for a logical MOBA pass.
#[derive(Clone, Copy)]
struct PhysicalDrawResource {
    pipeline: WorldModelPipelineHandle,
    texture_set: WorldModelTextureSetHandle,
}

/// One logical MOBA draw expanded to stock's one or two physical passes.
struct LogicalDrawResource {
    passes: [Option<PhysicalDrawResource>; 2],
    pass_count: usize,
}

/// One shared root/group WMO generation in renderer-local storage.
struct WorldModelGpuSource {
    plan: Arc<WorldModelMeshPlan>,
    mesh: WorldModelMeshHandle,
    draws: Vec<LogicalDrawResource>,
}

/// One MODF transform with retained visibility scratch.
struct WorldModelGpuPlacement {
    source_index: usize,
    plan: PlacedWorldModelDrawPlan,
    visible_draw_indices: Vec<usize>,
}

/// Complete resident WMO generation for one terrain tile.
pub(super) struct WorldModelFrame {
    sources: Vec<WorldModelGpuSource>,
    placements: Vec<WorldModelGpuPlacement>,
    prepared_draws: Vec<WorldModelPreparedDraw>,
}

impl WorldModelFrame {
    /// Uploads shared generations once and prepares each exact material pass.
    pub(super) fn prepare(
        renderer: &mut VulkanRenderer,
        scene: &ResidentWorldModelScene,
        filtering: WorldModelTextureFiltering,
        base_mip: WorldModelBaseMip,
    ) -> Result<Self, RuntimeTerrainFrameError> {
        let mut sources = Vec::with_capacity(scene.sources().len());
        for source in scene.sources() {
            let plan = Arc::new(WorldModelMeshPlan::prepare(source.model())?);
            let mesh = renderer.upload_world_model_mesh(&plan)?;
            let mut uploads = Vec::new();
            let mut needs_stock_green = false;
            for textures in source.materials() {
                match textures {
                    ResidentWorldModelMaterialTextures::One(texture) => {
                        collect_texture_upload(texture, &mut uploads, &mut needs_stock_green);
                    }
                    ResidentWorldModelMaterialTextures::Two(textures) => {
                        collect_texture_upload(&textures[0], &mut uploads, &mut needs_stock_green);
                        collect_texture_upload(&textures[1], &mut uploads, &mut needs_stock_green);
                    }
                }
            }
            let uploaded = renderer.upload_blp_textures(&uploads)?;
            let texture_handles = uploads
                .iter()
                .zip(uploaded)
                .map(|(request, handle)| (request.source().path().clone(), handle))
                .collect::<HashMap<_, _>>();
            let stock_green = if needs_stock_green {
                Some(renderer.upload_stock_world_model_green()?)
            } else {
                None
            };
            let mut texture_requests = Vec::with_capacity(source.materials().len());
            for (material, textures) in plan.materials().iter().zip(source.materials()) {
                let state = WorldModelMaterialState::from_material(material);
                let sampler = renderer.prepare_world_model_sampler(state, filtering, base_mip)?;
                texture_requests.push(match textures {
                    ResidentWorldModelMaterialTextures::One(texture) => {
                        WorldModelTextureSet::One(WorldModelSampledTexture::new(
                            resolve_texture(texture, &texture_handles, stock_green)?,
                            sampler,
                        ))
                    }
                    ResidentWorldModelMaterialTextures::Two(textures) => {
                        WorldModelTextureSet::Two([
                            WorldModelSampledTexture::new(
                                resolve_texture(&textures[0], &texture_handles, stock_green)?,
                                sampler,
                            ),
                            WorldModelSampledTexture::new(
                                resolve_texture(&textures[1], &texture_handles, stock_green)?,
                                sampler,
                            ),
                        ])
                    }
                });
            }
            if texture_requests.len() != plan.materials().len() {
                return Err(VulkanError::WorldModelDrawMaterial.into());
            }
            let texture_sets = if texture_requests.is_empty() {
                Vec::new()
            } else {
                renderer.prepare_world_model_texture_sets(&texture_requests)?
            };
            let draws = prepare_draw_resources(renderer, &plan, &texture_sets)?;
            sources.push(WorldModelGpuSource { plan, mesh, draws });
        }

        let mut placements = Vec::with_capacity(scene.placements().len());
        for placement in scene.placements() {
            let source = sources.get(placement.source_index()).ok_or(
                RuntimeTerrainFrameError::WorldModelSourceIndex {
                    source_index: placement.source_index(),
                    source_count: sources.len(),
                },
            )?;
            let plan = PlacedWorldModelDrawPlan::prepare(
                Arc::clone(&source.plan),
                placement.position(),
                placement.rotation_degrees(),
                1.0,
            )?;
            placements.push(WorldModelGpuPlacement {
                source_index: placement.source_index(),
                visible_draw_indices: Vec::with_capacity(source.plan.draws().len()),
                plan,
            });
        }
        let prepared_capacity = placements
            .iter()
            .map(|placement| {
                sources[placement.source_index]
                    .draws
                    .iter()
                    .map(|draw| draw.pass_count)
                    .sum::<usize>()
            })
            .sum::<usize>();
        Ok(Self {
            sources,
            placements,
            prepared_draws: Vec::with_capacity(prepared_capacity),
        })
    }

    /// Culls placements and replaces the retained physical packet buffer.
    pub(super) fn prepare_visible_draws(
        &mut self,
        renderer: &mut VulkanRenderer,
        frustum: WorldFrustum,
        environment_emissive: f32,
        fog_color: Vec3,
    ) -> Result<&[WorldModelPreparedDraw], RuntimeTerrainFrameError> {
        self.prepared_draws.clear();
        for placement in &mut self.placements {
            placement
                .plan
                .select_visible_draws(frustum, &mut placement.visible_draw_indices)?;
            let source = &self.sources[placement.source_index];
            for draw_index in placement.visible_draw_indices.iter().copied() {
                let resources =
                    source
                        .draws
                        .get(draw_index)
                        .ok_or(VulkanError::WorldModelDrawIndex {
                            requested: draw_index,
                            available: source.draws.len(),
                        })?;
                for pass_index in 0..resources.pass_count {
                    let resource =
                        resources.passes[pass_index].ok_or(VulkanError::WorldModelDrawPass {
                            requested: pass_index,
                            available: resources.pass_count,
                        })?;
                    self.prepared_draws.push(renderer.prepare_world_model_draw(
                        source.mesh,
                        resource.pipeline,
                        resource.texture_set,
                        &source.plan,
                        draw_index,
                        pass_index,
                        placement.plan.transform(),
                        environment_emissive,
                        fog_color,
                    )?);
                }
            }
        }
        Ok(&self.prepared_draws)
    }

    /// Returns the number of independently transformed MODF owners.
    pub(super) const fn placement_count(&self) -> usize {
        self.placements.len()
    }
}

fn prepare_draw_resources(
    renderer: &mut VulkanRenderer,
    plan: &WorldModelMeshPlan,
    texture_sets: &[WorldModelTextureSetHandle],
) -> Result<Vec<LogicalDrawResource>, RuntimeTerrainFrameError> {
    let mut resources = Vec::with_capacity(plan.draws().len());
    for draw in plan.draws() {
        let material = plan
            .materials()
            .get(usize::from(draw.material_id()))
            .ok_or(VulkanError::WorldModelDrawMaterial)?;
        let texture_set = texture_sets
            .get(usize::from(draw.material_id()))
            .copied()
            .ok_or(VulkanError::WorldModelDrawMaterial)?;
        let group_flags = plan
            .groups()
            .iter()
            .find(|group| group.group_index() == draw.group_index())
            .map(|group| group.flags())
            .ok_or(VulkanError::WorldModelDrawGroup)?;
        let pass_plan = WorldModelSurfacePassPlan::prepare(
            plan.root_flags(),
            group_flags,
            draw.class(),
            material,
        );
        let mut passes = [None; 2];
        for (pass_index, pass) in pass_plan.passes().iter().copied().enumerate() {
            passes[pass_index] = Some(PhysicalDrawResource {
                pipeline: renderer.prepare_world_model_pipeline(pass_plan.is_unified(), pass)?,
                texture_set,
            });
        }
        resources.push(LogicalDrawResource {
            passes,
            pass_count: pass_plan.passes().len(),
        });
    }
    Ok(resources)
}

fn collect_texture_upload<'source>(
    texture: &'source ResidentWorldModelTexture,
    uploads: &mut Vec<BlpTextureUploadRequest<'source>>,
    needs_stock_green: &mut bool,
) {
    match texture {
        ResidentWorldModelTexture::Authored(source) => {
            uploads.push(BlpTextureUploadRequest::new(source, BlpColorSpace::Srgb))
        }
        ResidentWorldModelTexture::StockGreen => *needs_stock_green = true,
    }
}

fn resolve_texture(
    texture: &ResidentWorldModelTexture,
    authored: &HashMap<solarity_asset::AssetPath, BlpTextureHandle>,
    stock_green: Option<BlpTextureHandle>,
) -> Result<BlpTextureHandle, RuntimeTerrainFrameError> {
    match texture {
        ResidentWorldModelTexture::Authored(source) => authored
            .get(source.path())
            .copied()
            .ok_or(VulkanError::UnknownWorldModelTextureHandle.into()),
        ResidentWorldModelTexture::StockGreen => {
            stock_green.ok_or(VulkanError::UnknownWorldModelTextureHandle.into())
        }
    }
}
