//! Renderer-local WMO resources and allocation-reusing MODF visibility.

mod streaming;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use glam::Vec3;
use solarity_ecs::WorldObjectIdentity;
use solarity_rendering::{
    BlpColorSpace, BlpTextureHandle, BlpTextureUploadRequest, PlacedWorldModelDrawPlan,
    VulkanError, VulkanRenderer, WorldFrustum, WorldModelBaseMip, WorldModelMaterialState,
    WorldModelMeshHandle, WorldModelMeshPlan, WorldModelPipelineHandle, WorldModelPreparedDraw,
    WorldModelSampledTexture, WorldModelSurfacePassPlan, WorldModelTextureFiltering,
    WorldModelTextureSet, WorldModelTextureSetHandle,
};

use crate::application::game_object_coordinator::{GameObjectFrameInput, GameObjectResource};
use crate::application::terrain_coordinator::world_model_residency::{
    ResidentWorldModelMaterialTextures, ResidentWorldModelScene, ResidentWorldModelSource,
    ResidentWorldModelTexture,
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
    model: Arc<solarity_asset::DecodedWorldModel>,
    plan: Arc<WorldModelMeshPlan>,
    mesh: WorldModelMeshHandle,
    draws: Vec<LogicalDrawResource>,
}

/// One MODF transform with retained visibility scratch.
struct WorldModelGpuPlacement {
    placement_valid: bool,
    source_index: usize,
    owner: WorldModelGpuPlacementOwner,
    plan: PlacedWorldModelDrawPlan,
    visible_draw_indices: Vec<usize>,
}

/// Placement identity used to retire only the dynamic transport generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WorldModelGpuPlacementOwner {
    /// One immutable terrain MODF owner.
    Static { unique_id: u32 },
    /// The controlled player's current movement parent.
    GameObject {
        identity: WorldObjectIdentity,
        display_id: u32,
    },
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
                placement_valid: true,
                source_index: placement.source_index(),
                owner: WorldModelGpuPlacementOwner::Static {
                    unique_id: placement.unique_id(),
                },
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

    /// Reconciles object lifetimes while reusing shared WMO GPU generations.
    pub(super) fn synchronize_game_objects(
        &mut self,
        renderer: &mut VulkanRenderer,
        game_objects: GameObjectFrameInput<'_>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let sources = &self.sources;
        self.placements.retain(|placement| {
            let WorldModelGpuPlacementOwner::GameObject {
                identity,
                display_id,
            } = placement.owner
            else {
                return true;
            };
            game_objects.get(identity).is_some_and(|instance| {
                instance.display_id() == display_id
                    && match (
                        instance.resource(),
                        sources.get(placement.source_index).and_then(Option::as_ref),
                    ) {
                        (Some(GameObjectResource::WorldModel(cpu)), Some(gpu)) => {
                            Arc::ptr_eq(cpu.model(), &gpu.model)
                        }
                        _ => false,
                    }
            })
        });
        let retained = self
            .placements
            .iter()
            .filter_map(|placement| match placement.owner {
                WorldModelGpuPlacementOwner::GameObject {
                    identity,
                    display_id,
                } => Some((identity, display_id)),
                _ => None,
            })
            .collect::<HashSet<_>>();
        let mut sources = self
            .sources
            .iter()
            .enumerate()
            .filter_map(|(index, source)| {
                source
                    .as_ref()
                    .map(|source| (Arc::as_ptr(&source.model), index))
            })
            .collect::<HashMap<_, _>>();
        for instance in game_objects.instances() {
            if retained.contains(&(instance.identity(), instance.display_id())) {
                continue;
            }
            let (Some(GameObjectResource::WorldModel(cpu)), Some(resolved)) =
                (instance.resource(), instance.placement())
            else {
                continue;
            };
            let source_index = if let Some(index) = sources.get(&Arc::as_ptr(cpu.model())) {
                *index
            } else {
                let gpu = prepare_gpu_source(renderer, cpu, self.filtering, self.base_mip)?;
                let index = self.sources.len();
                sources.insert(Arc::as_ptr(&gpu.model), index);
                self.sources.push(Some(gpu));
                index
            };
            let gpu = self.sources[source_index].as_ref().ok_or(
                RuntimeTerrainFrameError::WorldModelSourceIndex {
                    source_index,
                    source_count: self.sources.len(),
                },
            )?;
            self.placements.push(WorldModelGpuPlacement {
                placement_valid: true,
                source_index,
                owner: WorldModelGpuPlacementOwner::GameObject {
                    identity: instance.identity(),
                    display_id: instance.display_id(),
                },
                plan: PlacedWorldModelDrawPlan::prepare_with_transform(
                    Arc::clone(&gpu.plan),
                    resolved.matrix(),
                )?,
                visible_draw_indices: Vec::with_capacity(gpu.plan.draws().len()),
            });
        }
        self.compact_sources();
        Ok(())
    }

    /// Updates bounds in place; unresolved parents hide an existing owner.
    pub(super) fn update_game_object_states(
        &mut self,
        game_objects: GameObjectFrameInput<'_>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        for placement in &mut self.placements {
            let WorldModelGpuPlacementOwner::GameObject { identity, .. } = placement.owner else {
                continue;
            };
            let resolved = game_objects
                .get(identity)
                .and_then(|instance| instance.placement());
            placement.placement_valid = resolved.is_some();
            if let Some(resolved) = resolved {
                placement.plan.set_transform(resolved.matrix())?;
            }
        }
        Ok(())
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
            if !placement.placement_valid {
                continue;
            }
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
    Ok(WorldModelGpuSource {
        model: Arc::clone(source.model()),
        plan,
        mesh,
        draws,
    })
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
