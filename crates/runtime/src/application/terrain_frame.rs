//! Resident ADT composition into immutable renderer-owned resources.

use std::sync::Arc;

use solarity_asset::{AssetPath, BlpTextureSource, TerrainTileIndex};
use solarity_rendering::{
    BlpColorSpace, BlpTextureUploadError, TerrainFrameReport, TerrainLayerCount,
    TerrainLayerCountError, TerrainPreparedDraw, TerrainSceneUniform, TerrainTextureSet,
    TerrainTileMeshPlan, VulkanError, VulkanRenderer, WorldCameraError, WorldCameraFrame,
    WorldFrustum, WorldScreenWindow,
};
use thiserror::Error;

use crate::application::environment_coordinator::RuntimeWorldEnvironmentFrame;

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
}

/// One immutable resident ADT generation ready for camera selection.
pub(super) struct TerrainFrame {
    tile: TerrainTileIndex,
    draws: Vec<TerrainPreparedDraw>,
    visible_draws: Vec<TerrainPreparedDraw>,
}

impl TerrainFrame {
    /// Uploads and validates every resource referenced by one admitted ADT.
    pub(super) fn prepare(
        renderer: &mut VulkanRenderer,
        plan: &TerrainTileMeshPlan,
        sources: &[Arc<BlpTextureSource>],
    ) -> Result<Self, RuntimeTerrainFrameError> {
        validate_texture_table(plan, sources)?;

        // The shared tile allocations are immutable. Renderer registries use
        // the plan identity to suppress duplicate staging work within a
        // generation, while MTEX images deduplicate by selected asset identity.
        let mesh = renderer.upload_terrain_mesh(plan)?;
        let material = renderer.upload_terrain_material(plan)?;
        let textures = sources
            .iter()
            .map(|source| renderer.upload_blp_texture(source, BlpColorSpace::Srgb))
            .collect::<Result<Vec<_>, _>>()?;

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
    ) -> Result<TerrainFrameReport, RuntimeTerrainFrameError> {
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
        let scene = TerrainSceneUniform::new(
            camera.view_projection(),
            light.ambient_color(),
            light.diffuse_color(),
            environment.light_direction(),
        );
        Ok(renderer.present_terrain(scene, &self.visible_draws)?)
    }

    /// Returns the ADT whose renderer resources this generation represents.
    pub(super) const fn tile(&self) -> TerrainTileIndex {
        self.tile
    }

    /// Returns the complete row-major packet count retained for camera culling.
    pub(super) fn draw_count(&self) -> usize {
        self.draws.len()
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
