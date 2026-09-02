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
    ResidentWorldModelMaterialTextures, ResidentWorldModelScene, ResidentWorldModelSource,
    ResidentWorldModelTexture,
};
use crate::application::transport_coordinator::{ResidentTransport, ResidentTransportResource};

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
    owner: WorldModelGpuPlacementOwner,
    plan: PlacedWorldModelDrawPlan,
    visible_draw_indices: Vec<usize>,
}

/// Placement identity used to retire only the dynamic transport generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WorldModelGpuPlacementOwner {
    /// One immutable terrain MODF owner.
    Static,
    /// The controlled player's current movement parent.
    Transport { guid: u64 },
}

/// Complete resident WMO generation for one terrain tile.
pub(super) struct WorldModelFrame {
    sources: Vec<Option<WorldModelGpuSource>>,
    placements: Vec<WorldModelGpuPlacement>,
    prepared_draws: Vec<WorldModelPreparedDraw>,
    filtering: WorldModelTextureFiltering,
    base_mip: WorldModelBaseMip,
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
            sources.push(Some(prepare_gpu_source(
                renderer, source, filtering, base_mip,
            )?));
        }

        let mut placements = Vec::with_capacity(scene.placements().len());
        for placement in scene.placements() {
            let source = sources
                .get(placement.source_index())
                .and_then(Option::as_ref)
                .ok_or(RuntimeTerrainFrameError::WorldModelSourceIndex {
                    source_index: placement.source_index(),
                    source_count: sources.len(),
                })?;
            let plan = PlacedWorldModelDrawPlan::prepare(
                Arc::clone(&source.plan),
                placement.position(),
                placement.rotation_degrees(),
                1.0,
            )?;
            placements.push(WorldModelGpuPlacement {
                source_index: placement.source_index(),
                owner: WorldModelGpuPlacementOwner::Static,
                visible_draw_indices: Vec::with_capacity(source.plan.draws().len()),
                plan,
            });
        }
        let mut prepared_capacity = 0;
        for placement in &placements {
            let source = sources[placement.source_index].as_ref().ok_or(
                RuntimeTerrainFrameError::WorldModelSourceIndex {
                    source_index: placement.source_index,
                    source_count: sources.len(),
                },
            )?;
            prepared_capacity += source
                .draws
                .iter()
                .map(|draw| draw.pass_count)
                .sum::<usize>();
        }
        Ok(Self {
            sources,
            placements,
            prepared_draws: Vec::with_capacity(prepared_capacity),
            filtering,
            base_mip,
        })
    }

    /// Replaces the dynamic movement-parent WMO without disturbing MODF state.
    pub(super) fn replace_transport(
        &mut self,
        renderer: &mut VulkanRenderer,
        transport: Option<&ResidentTransport>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let prepared = match transport {
            Some(transport) => match (
                transport.resource(),
                transport.transform(),
                transport.scale(),
            ) {
                (ResidentTransportResource::WorldModel(source), Some(transform), Some(scale)) => {
                    let gpu = prepare_gpu_source(renderer, source, self.filtering, self.base_mip)?;
                    let plan = transport_draw_plan(&gpu.plan, transform, scale)?;
                    Some((transport.guid(), gpu, plan))
                }
                _ => None,
            },
            None => None,
        };

        self.remove_transport();
        if let Some((guid, source, plan)) = prepared {
            let source_index = self.sources.len();
            let visible_draw_indices = Vec::with_capacity(source.plan.draws().len());
            self.sources.push(Some(source));
            self.placements.push(WorldModelGpuPlacement {
                source_index,
                owner: WorldModelGpuPlacementOwner::Transport { guid },
                plan,
                visible_draw_indices,
            });
        }
        Ok(())
    }

    /// Applies the latest replicated transport transform to the retained WMO.
    pub(super) fn update_transport_state(
        &mut self,
        transport: Option<&ResidentTransport>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let Some(transport) = transport else {
            return Ok(());
        };
        let (ResidentTransportResource::WorldModel(_), Some(transform), Some(scale)) = (
            transport.resource(),
            transport.transform(),
            transport.scale(),
        ) else {
            return Ok(());
        };
        let Some(placement) = self.placements.iter_mut().find(|placement| {
            placement.owner
                == WorldModelGpuPlacementOwner::Transport {
                    guid: transport.guid(),
                }
        }) else {
            return Ok(());
        };
        let source = self.sources[placement.source_index].as_ref().ok_or(
            RuntimeTerrainFrameError::WorldModelSourceIndex {
                source_index: placement.source_index,
                source_count: self.sources.len(),
            },
        )?;
        placement.plan = transport_draw_plan(&source.plan, transform, scale)?;
        Ok(())
    }

    /// Retires only the renderer references owned by the current transport.
    fn remove_transport(&mut self) {
        let mut source_indices = Vec::new();
        self.placements.retain(|placement| {
            if matches!(
                placement.owner,
                WorldModelGpuPlacementOwner::Transport { .. }
            ) {
                source_indices.push(placement.source_index);
                false
            } else {
                true
            }
        });
        for source_index in source_indices {
            if let Some(source) = self.sources.get_mut(source_index) {
                *source = None;
            }
        }
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
            let source = self.sources[placement.source_index].as_ref().ok_or(
                RuntimeTerrainFrameError::WorldModelSourceIndex {
                    source_index: placement.source_index,
                    source_count: self.sources.len(),
                },
            )?;
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

/// Uploads one retained root/group generation and its exact material stages.
fn prepare_gpu_source(
    renderer: &mut VulkanRenderer,
    source: &ResidentWorldModelSource,
    filtering: WorldModelTextureFiltering,
    base_mip: WorldModelBaseMip,
) -> Result<WorldModelGpuSource, RuntimeTerrainFrameError> {
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
            ResidentWorldModelMaterialTextures::Two(textures) => WorldModelTextureSet::Two([
                WorldModelSampledTexture::new(
                    resolve_texture(&textures[0], &texture_handles, stock_green)?,
                    sampler,
                ),
                WorldModelSampledTexture::new(
                    resolve_texture(&textures[1], &texture_handles, stock_green)?,
                    sampler,
                ),
            ]),
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
    Ok(WorldModelGpuSource { plan, mesh, draws })
}

/// Converts ECS Z-up yaw using `world_game_object_projector.cpp::makeWmo`.
fn transport_draw_plan(
    source: &Arc<WorldModelMeshPlan>,
    transform: solarity_ecs::WorldTransform,
    scale: f32,
) -> Result<PlacedWorldModelDrawPlan, RuntimeTerrainFrameError> {
    Ok(PlacedWorldModelDrawPlan::prepare(
        Arc::clone(source),
        transform.position(),
        Vec3::new(0.0, transform.orientation().to_degrees() - 180.0, 0.0),
        scale,
    )?)
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
