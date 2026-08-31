//! Resident ADT composition into immutable renderer-owned resources.

use std::sync::Arc;

use glam::Vec4;
use solarity_asset::{AssetPath, BlpTextureSource, TerrainTileIndex};
use solarity_rendering::{
    BlpColorSpace, BlpTextureUploadError, M2BonePoseError, M2LocalLightState, M2MaterialPoseError,
    M2MeshPlanError, M2SceneUniform, M2ShaderPlanError, TerrainLayerCount, TerrainLayerCountError,
    TerrainPreparedDraw, TerrainSceneUniform, TerrainTextureSet, TerrainTileMeshPlan, VulkanError,
    VulkanRenderer, WorldCameraError, WorldCameraFrame, WorldFrameReport, WorldFrameScene,
    WorldFrustum, WorldModelBaseMip, WorldModelMeshPlanError, WorldModelPlacementError,
    WorldModelSceneUniform, WorldModelTextureFiltering, WorldScreenWindow,
};
use thiserror::Error;

use crate::application::environment_coordinator::RuntimeWorldEnvironmentFrame;
use crate::application::terrain_coordinator::m2_residency::ResidentM2Scene;
use crate::application::terrain_coordinator::world_model_residency::ResidentWorldModelScene;
use crate::random::CrtRand;

mod m2;
mod world_model;

use m2::M2Frame;
use world_model::WorldModelFrame;

/// Failure while joining a resident ADT to renderer-local GPU resources.
#[derive(Debug, Error)]
pub enum RuntimeTerrainFrameError {
    /// An authored BLP could not decode or enter device-local storage.
    #[error(transparent)]
    TextureUpload(#[from] BlpTextureUploadError),
    /// Vulkan resource preparation or cross-resource validation failed.
    #[error(transparent)]
    Vulkan(#[from] VulkanError),
    /// An MCNK carried a diffuse-layer count outside stock's closed domain.
    #[error(transparent)]
    LayerCount(#[from] TerrainLayerCountError),
    /// The final player camera could not form a valid frustum.
    #[error(transparent)]
    Camera(#[from] WorldCameraError),
    /// A decoded WMO could not enter the combined direct-index mesh ABI.
    #[error(transparent)]
    WorldModelMesh(#[from] WorldModelMeshPlanError),
    /// An authored MODF transform could not enter presentation state.
    #[error(transparent)]
    WorldModelPlacement(#[from] WorldModelPlacementError),
    /// An M2 SKIN profile could not enter the direct-index mesh ABI.
    #[error(transparent)]
    M2Mesh(#[from] M2MeshPlanError),
    /// One M2 material batch could not select a stock BLS effect.
    #[error(transparent)]
    M2Shader(#[from] M2ShaderPlanError),
    /// One visible M2 could not form its bone palette.
    #[error(transparent)]
    M2BonePose(#[from] M2BonePoseError),
    /// One visible M2 material could not sample its authored animation tracks.
    #[error(transparent)]
    M2MaterialPose(#[from] M2MaterialPoseError),
    /// A runtime-alpha mesh lacked the pipeline prepared for stock promotion.
    #[error("M2 model {model} draw {draw_index} has no runtime-alpha fade pipeline")]
    M2RuntimeFadePipeline {
        /// Model whose animated material entered the transparent pass.
        model: AssetPath,
        /// Zero-based draw within the selected SKIN profile.
        draw_index: usize,
    },
    /// The retained MTEX sources no longer match the immutable mesh plan.
    #[error(
        "terrain MTEX source count {source_count} does not match mesh texture count {plan_count}"
    )]
    TextureTableCount {
        /// Number of retained decoded BLP sources.
        source_count: usize,
        /// Number of paths captured by mesh preparation.
        plan_count: usize,
    },
    /// One retained BLP source does not occupy its authored MTEX slot.
    #[error("terrain MTEX source {index} is {actual}, expected {expected}")]
    TextureTablePath {
        /// Authored MTEX table index.
        index: usize,
        /// Path captured by mesh preparation.
        expected: AssetPath,
        /// Path attached to the retained BLP source.
        actual: AssetPath,
    },
    /// An MCNK layer references a texture absent from the retained MTEX table.
    #[error(
        "terrain chunk {chunk_index} references MTEX index {texture_index}, but only {texture_count} textures are resident"
    )]
    TextureIndex {
        /// Row-major MCNK index in the ADT.
        chunk_index: usize,
        /// Invalid authored MTEX index.
        texture_index: u32,
        /// Number of resident MTEX textures.
        texture_count: usize,
    },
    /// Terrain reported a new resident tile without its immutable mesh plan.
    #[error("resident terrain tile [{tile_x}, {tile_y}] has no mesh plan")]
    MissingMeshPlan {
        /// Resident ADT X coordinate.
        tile_x: u8,
        /// Resident ADT Y coordinate.
        tile_y: u8,
    },
    /// Terrain reported a new resident tile without its retained MTEX sources.
    #[error("resident terrain tile [{tile_x}, {tile_y}] has no MTEX sources")]
    MissingTextureSources {
        /// Resident ADT X coordinate.
        tile_x: u8,
        /// Resident ADT Y coordinate.
        tile_y: u8,
    },
    /// Terrain reported a new tile without its admitted WMO presentation scene.
    #[error("resident terrain tile [{tile_x}, {tile_y}] has no WMO presentation scene")]
    MissingWorldModelScene {
        /// Resident ADT X coordinate.
        tile_x: u8,
        /// Resident ADT Y coordinate.
        tile_y: u8,
    },
    /// Terrain reported a new tile without its admitted M2 presentation scene.
    #[error("resident terrain tile [{tile_x}, {tile_y}] has no M2 presentation scene")]
    MissingM2Scene {
        /// Resident ADT X coordinate.
        tile_x: u8,
        /// Resident ADT Y coordinate.
        tile_y: u8,
    },
    /// Terrain residency and the retained GPU generation became inconsistent.
    #[error("current terrain tile [{tile_x}, {tile_y}] has no matching GPU generation")]
    MissingGpuGeneration {
        /// Resident ADT X coordinate.
        tile_x: u8,
        /// Resident ADT Y coordinate.
        tile_y: u8,
    },
    /// A retained GPU generation was paired with another resident tile plan.
    #[error("terrain GPU tile [{frame_x}, {frame_y}] does not match plan [{plan_x}, {plan_y}]")]
    TileMismatch {
        /// Retained GPU generation X coordinate.
        frame_x: u8,
        /// Retained GPU generation Y coordinate.
        frame_y: u8,
        /// Submitted CPU plan X coordinate.
        plan_x: u8,
        /// Submitted CPU plan Y coordinate.
        plan_y: u8,
    },
    /// One retained MODF references no resident source-table slot.
    #[error(
        "resident WMO placement references source {source_index}, but only {source_count} sources exist"
    )]
    WorldModelSourceIndex {
        /// Invalid source-table slot.
        source_index: usize,
        /// Number of retained root-WMO generations.
        source_count: usize,
    },
    /// One placed MDDF/MODD instance references no resident M2 source slot.
    #[error(
        "resident M2 placement references source {source_index}, but only {source_count} sources exist"
    )]
    M2SourceIndex {
        /// Invalid M2 source-table slot.
        source_index: usize,
        /// Number of resident M2 generations.
        source_count: usize,
    },
    /// Resident M2 texture sources no longer parallel the model declarations.
    #[error("M2 {model} retains {source_count} texture sources for {model_count} declarations")]
    M2TextureTableCount {
        /// Model whose immutable texture table became inconsistent.
        model: AssetPath,
        /// Number of resident texture sources.
        source_count: usize,
        /// Number of decoded model texture declarations.
        model_count: usize,
    },
    /// A validated M2 draw references no corresponding resident texture source.
    #[error("M2 {model} draw references absent resident texture {texture_index}")]
    M2TextureIndex {
        /// Model containing the selected material draw.
        model: AssetPath,
        /// Texture-declaration index selected through the SKIN combo table.
        texture_index: u16,
    },
    /// A selected M2 animation no longer resolves through its validated aliases.
    #[error("M2 {model} selected absent animation sequence {sequence}")]
    M2SequenceIndex {
        /// Model whose immutable animation catalog became inconsistent.
        model: AssetPath,
        /// Selected sequence-table index.
        sequence: usize,
    },
    /// A previously admitted M2 animation can no longer select a variation.
    #[error("M2 {model} cannot select animation ID {animation_id}")]
    M2AnimationSelection {
        /// Model whose authored variation chain became unavailable.
        model: AssetPath,
        /// AnimationData identifier being advanced.
        animation_id: u16,
    },
}

/// One immutable resident ADT generation ready for camera selection.
pub(super) struct TerrainFrame {
    tile: TerrainTileIndex,
    draws: Vec<TerrainPreparedDraw>,
    visible_draws: Vec<TerrainPreparedDraw>,
    m2: M2Frame,
    world_models: WorldModelFrame,
}

impl TerrainFrame {
    /// Uploads and validates every resource referenced by one admitted ADT.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare(
        renderer: &mut VulkanRenderer,
        plan: &TerrainTileMeshPlan,
        sources: &[Arc<BlpTextureSource>],
        m2_scene: &ResidentM2Scene,
        world_models: &ResidentWorldModelScene,
        world_model_filtering: WorldModelTextureFiltering,
        world_model_base_mip: WorldModelBaseMip,
        random: &mut CrtRand,
    ) -> Result<Self, RuntimeTerrainFrameError> {
        validate_texture_table(plan, sources)?;

        // The shared tile allocations are immutable. Renderer registries use
        // the plan identity to suppress duplicate staging work within a
        // generation, while MTEX images deduplicate by selected asset identity.
        let mesh = renderer.upload_terrain_mesh(plan)?;
        let material = renderer.upload_terrain_material(plan)?;
        let texture_uploads = sources
            .iter()
            .map(|source| {
                solarity_rendering::BlpTextureUploadRequest::new(source, BlpColorSpace::Srgb)
            })
            .collect::<Vec<_>>();
        let textures = renderer.upload_blp_textures(&texture_uploads)?;

        let mut requests = Vec::with_capacity(plan.chunks().len());
        let mut layer_counts = Vec::with_capacity(plan.chunks().len());
        for (chunk_index, chunk) in plan.chunks().iter().enumerate() {
            let layer_count = TerrainLayerCount::try_from(chunk.layers().len())?;
            let layers = chunk
                .layers()
                .iter()
                .map(|layer| {
                    let texture_index =
                        usize::try_from(layer.texture_index()).map_err(|_source| {
                            RuntimeTerrainFrameError::TextureIndex {
                                chunk_index,
                                texture_index: layer.texture_index(),
                                texture_count: textures.len(),
                            }
                        })?;
                    textures.get(texture_index).copied().ok_or(
                        RuntimeTerrainFrameError::TextureIndex {
                            chunk_index,
                            texture_index: layer.texture_index(),
                            texture_count: textures.len(),
                        },
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            requests.push(TerrainTextureSet::new(material, &layers)?);
            layer_counts.push(layer_count);
        }

        // Descriptor allocation is exact-demand and internally shares
        // identical atlas/layer tuples. Pipeline preparation similarly reduces
        // the 256 chunks to the one-through-four variants actually authored.
        let texture_sets = renderer.prepare_terrain_texture_sets(&requests)?;
        let mut pipelines = [None; 4];
        for layer_count in &layer_counts {
            let slot = usize::from(layer_count.get() - 1);
            if pipelines[slot].is_none() {
                pipelines[slot] = Some(renderer.prepare_terrain_pipeline(*layer_count)?);
            }
        }

        let mut draws = Vec::with_capacity(plan.chunks().len());
        for chunk_index in 0..plan.chunks().len() {
            let slot = usize::from(layer_counts[chunk_index].get() - 1);
            let Some(pipeline) = pipelines[slot] else {
                // The immediately preceding loop prepared this exact closed
                // layer-count slot, so absence would indicate local corruption.
                return Err(RuntimeTerrainFrameError::Vulkan(
                    VulkanError::TerrainDrawPipelineMismatch,
                ));
            };
            draws.push(renderer.prepare_terrain_draw(
                mesh,
                pipeline,
                texture_sets[chunk_index],
                &requests[chunk_index],
                plan,
                chunk_index,
            )?);
        }

        Ok(Self {
            tile: plan.tile(),
            draws,
            visible_draws: Vec::with_capacity(plan.chunks().len()),
            m2: M2Frame::prepare(renderer, m2_scene, random)?,
            world_models: WorldModelFrame::prepare(
                renderer,
                world_models,
                world_model_filtering,
                world_model_base_mip,
            )?,
        })
    }

    /// Culls, lights, records, and presents one resident terrain frame.
    ///
    /// The visible packet buffer is retained across frames. An empty result is
    /// still presented as a cleared world attachment; looking away from the
    /// resident ADT is valid camera state, not a rendering failure.
    pub(super) fn present(
        &mut self,
        renderer: &mut VulkanRenderer,
        plan: &TerrainTileMeshPlan,
        environment: RuntimeWorldEnvironmentFrame,
        camera: WorldCameraFrame,
        global_animation_time_ms: f32,
        random: &mut CrtRand,
    ) -> Result<WorldFrameReport, RuntimeTerrainFrameError> {
        if self.tile != plan.tile() {
            return Err(RuntimeTerrainFrameError::TileMismatch {
                frame_x: self.tile.x(),
                frame_y: self.tile.y(),
                plan_x: plan.tile().x(),
                plan_y: plan.tile().y(),
            });
        }
        let frustum = WorldFrustum::new(camera, WorldScreenWindow::FULL)?;
        self.visible_draws.clear();
        for (chunk, draw) in plan.chunks().iter().zip(&self.draws) {
            if chunk.is_visible(frustum)? {
                self.visible_draws.push(*draw);
            }
        }
        let light = environment.light();
        let terrain_scene = TerrainSceneUniform::new(
            camera.view_projection(),
            light.ambient_color(),
            light.diffuse_color(),
            environment.light_direction(),
        );
        let (fog_start, fog_end) = light.fog_range();
        let fog_parameters = Vec4::new(fog_start, fog_end, 0.0, 1.0);
        let world_model_scene = WorldModelSceneUniform::new(
            camera.view_projection(),
            camera.camera().position(),
            light.ambient_color(),
            light.diffuse_color(),
            environment.light_direction(),
            fog_parameters,
        );
        let m2_scene = M2SceneUniform::new(
            camera.view_projection(),
            camera.camera().position(),
            light.ambient_color(),
            light.diffuse_color(),
            environment.light_direction(),
            fog_parameters,
            [M2LocalLightState::disabled(); 4],
        );
        let scene = WorldFrameScene::new(terrain_scene, world_model_scene, m2_scene);
        let world_model_draws = self.world_models.prepare_visible_draws(
            renderer,
            frustum,
            environment.world_model_emissive(),
            light.fog_color(),
        )?;
        let local_animation_time_ms = self.m2.animation_time_ms();
        let (m2_bones, m2_draws) = self.m2.prepare_visible_draws(
            renderer,
            frustum,
            camera,
            light.fog_color(),
            local_animation_time_ms,
            global_animation_time_ms,
            random,
        )?;
        Ok(renderer.present_world_frame(
            scene,
            m2_bones,
            &self.visible_draws,
            world_model_draws,
            m2_draws,
        )?)
    }

    /// Returns the ADT whose renderer resources this generation represents.
    pub(super) const fn tile(&self) -> TerrainTileIndex {
        self.tile
    }

    /// Returns the complete row-major packet count retained for camera culling.
    pub(super) fn draw_count(&self) -> usize {
        self.draws.len()
    }

    /// Returns the number of independently transformed resident WMO owners.
    pub(super) fn world_model_placement_count(&self) -> usize {
        self.world_models.placement_count()
    }

    /// Returns the number of selected shared M2 GPU generations.
    pub(super) fn m2_mesh_count(&self) -> usize {
        self.m2.mesh_count()
    }

    /// Returns the number of retained MDDF and MODD instances.
    pub(super) fn m2_placement_count(&self) -> usize {
        self.m2.placement_count()
    }
}

/// Proves that mesh and source tables describe one exact MTEX generation.
fn validate_texture_table(
    plan: &TerrainTileMeshPlan,
    sources: &[Arc<BlpTextureSource>],
) -> Result<(), RuntimeTerrainFrameError> {
    if sources.len() != plan.textures().len() {
        return Err(RuntimeTerrainFrameError::TextureTableCount {
            source_count: sources.len(),
            plan_count: plan.textures().len(),
        });
    }
    for (index, (expected, source)) in plan.textures().iter().zip(sources).enumerate() {
        if expected != source.path() {
            return Err(RuntimeTerrainFrameError::TextureTablePath {
                index,
                expected: expected.clone(),
                actual: source.path().clone(),
            });
        }
    }
    Ok(())
}
