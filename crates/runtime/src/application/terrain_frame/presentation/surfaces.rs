//! Surface consumers use published light sources independently of M2 uniforms.

use super::super::{RuntimeTerrainFrameError, TerrainGpuTile, exterior, m2, world_model};
use crate::application::terrain_coordinator::RuntimeTerrainCoordinator;
use glam::Vec3;
use solarity_rendering::{
    LiquidFog, LiquidLighting, LiquidPreparedDraw, TerrainPreparedDraw, VulkanRenderer,
    WorldCameraFrame, WorldFrustum,
};

/// Main-owned packet storage is disjoint from the pending M2 continuation.
pub(super) struct SurfacePreparation<'frame> {
    pub tiles: &'frame [TerrainGpuTile],
    pub terrain_draws: &'frame mut Vec<TerrainPreparedDraw>,
    pub liquid_draws: &'frame mut Vec<LiquidPreparedDraw>,
    pub world_models: &'frame world_model::WorldModelFrame,
}

/// Frozen camera and environment values are shared by both liquid domains.
pub(super) struct SurfaceInputs {
    pub exterior_frustum: Option<WorldFrustum>,
    pub camera: WorldCameraFrame,
    pub lighting: LiquidLighting,
    pub fog: LiquidFog,
    pub ordinary_model_fog: Vec3,
    pub liquid_time_ms: u32,
    pub specular_enabled: bool,
}

impl SurfacePreparation<'_> {
    /// Runs terrain then liquid packet work exactly once after light publication.
    /// The outer driver retains failures until M2's original consumption boundary.
    pub(super) fn prepare(
        self,
        renderer: &VulkanRenderer,
        terrain: &RuntimeTerrainCoordinator,
        input: SurfaceInputs,
        lights: m2::SceneLightInputs<'_>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let mut profile = solarity_profiling::profile!("world.lit_surfaces");
        let _trace = solarity_profiling::TraceSpan::new("world.lit_surfaces.consume", 0, 0);
        lights.trace.link("m2.light_sources.consume");
        exterior::prepare_terrain_draws(
            self.tiles,
            self.terrain_draws,
            input.exterior_frustum,
            input.camera,
            lights.points,
        )?;
        profile.mark("terrain culling and point lights");
        self.liquid_draws.clear();
        for (frustum, batch) in input.exterior_frustum.into_iter().flat_map(|frustum| {
            self.tiles
                .iter()
                .flat_map(|tile| &tile.liquids)
                .map(move |batch| (frustum, batch))
        }) {
            if let Some(draw) = batch.prepare_draw(
                renderer,
                frustum,
                input.camera,
                input.lighting,
                input.fog,
                input.liquid_time_ms,
                input.specular_enabled,
                Some((lights.points, lights.directionals)),
            )? {
                self.liquid_draws.push(draw);
            }
        }
        self.world_models.prepare_liquid_draws(
            renderer,
            terrain.world_model_scene_groups(),
            input.camera,
            input.lighting,
            input.fog,
            input.ordinary_model_fog,
            input.liquid_time_ms,
            input.specular_enabled,
            Some((lights.points, lights.directionals)),
            self.liquid_draws,
        )?;
        profile.mark("liquid packets");
        Ok(())
    }
}
