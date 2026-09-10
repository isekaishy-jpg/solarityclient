//! Public scene snapshot and observable unified-world submission facts.

use crate::{
    LiquidFrame, M2SceneLightBank, M2SceneUniform, TerrainSceneUniform, UnderwaterParticleFrame,
    WaterRippleFrame, WorldCloudFrame, WorldModelSceneUniform, WorldSkyFrame,
};

/// One coherent terrain, WMO, M2, and M2-effect scene snapshot for a world frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldFrameScene<'a> {
    terrain: TerrainSceneUniform,
    world_model: WorldModelSceneUniform,
    m2: [M2SceneUniform; M2SceneLightBank::COUNT],
    m2_instance_scenes: &'a [M2SceneUniform],
    particle_vertex_capacity: usize,
    particle_index_capacity: usize,
    liquids: Option<LiquidFrame<'a>>,
    ripples: Option<WaterRippleFrame<'a>>,
    underwater: Option<UnderwaterParticleFrame<'a>>,
    sky: Option<WorldSkyFrame<'a>>,
    low_detail: Option<crate::WorldLowDetailFrame<'a>>,
    ground_detail: Option<crate::GroundDetailFrame<'a>>,
    primary_shadows: Option<crate::WorldPrimaryShadowFrame<'a>>,
    world_depth_range: bool,
    clouds: Option<WorldCloudFrame<'a>>,
    celestials: Option<crate::WorldCelestialFrame<'a>>,
    sky_models: Option<WorldSkyModelFrame<'a>>,
    sky_window: Option<crate::WorldSkyWindow>,
    background_color: glam::Vec4,
}

impl<'a> WorldFrameScene<'a> {
    /// Adds the primary unit-shadow caster pass before the terrain receiver queue.
    #[must_use]
    pub const fn with_primary_shadows(mut self, frame: crate::WorldPrimaryShadowFrame<'a>) -> Self {
        self.primary_shadows = Some(frame);
        self
    }

    pub(in crate::device) const fn primary_shadows(
        self,
    ) -> Option<crate::WorldPrimaryShadowFrame<'a>> {
        self.primary_shadows
    }

    /// Joins the three stock world shader families at one camera/time sample.
    #[must_use]
    pub const fn new(
        terrain: TerrainSceneUniform,
        world_model: WorldModelSceneUniform,
        m2: M2SceneUniform,
    ) -> Self {
        Self {
            terrain,
            world_model,
            m2: [m2; M2SceneLightBank::COUNT],
            m2_instance_scenes: &[],
            particle_vertex_capacity: 0,
            particle_index_capacity: 0,
            liquids: None,
            ripples: None,
            underwater: None,
            sky: None,
            low_detail: None,
            ground_detail: None,
            primary_shadows: None,
            world_depth_range: false,
            clouds: None,
            celestials: None,
            sky_models: None,
            sky_window: Some(crate::WorldSkyWindow::FULL),
            background_color: glam::Vec4::new(0., 0., 0., 1.),
        }
    }

    /// Selects stock's ordinary world depth interval, below horizon and sky.
    #[must_use]
    pub const fn with_world_depth_range(mut self) -> Self {
        self.world_depth_range = true;
        self
    }

    /// Adds map-wide WDL terrain between the sky and ordinary world queues.
    #[must_use]
    pub const fn with_low_detail(mut self, frame: crate::WorldLowDetailFrame<'a>) -> Self {
        self.low_detail = Some(frame);
        self.world_depth_range = true;
        self
    }

    pub(in crate::device) const fn low_detail(self) -> Option<crate::WorldLowDetailFrame<'a>> {
        self.low_detail
    }

    /// Adds the native terrain-detail queue using the shared outdoor scene state.
    #[must_use]
    pub const fn with_ground_detail(mut self, frame: crate::GroundDetailFrame<'a>) -> Self {
        self.ground_detail = Some(frame);
        self
    }

    pub(in crate::device) const fn ground_detail(self) -> Option<crate::GroundDetailFrame<'a>> {
        self.ground_detail
    }

    pub(in crate::device) const fn depth_maximum(self) -> f32 {
        if self.world_depth_range {
            crate::WORLD_DEPTH_MAXIMUM
        } else {
            1.0
        }
    }

    /// Supplies independent lighting for world model instances and their effects.
    #[must_use]
    pub const fn with_m2_instance_scenes(mut self, scenes: &'a [M2SceneUniform]) -> Self {
        self.m2_instance_scenes = scenes;
        self
    }

    pub(super) const fn m2_instance_scenes(self) -> &'a [M2SceneUniform] {
        self.m2_instance_scenes
    }

    /// Adds camera-relative authored models in the two native sky queues.
    #[must_use]
    pub const fn with_sky_models(mut self, frame: WorldSkyModelFrame<'a>) -> Self {
        self.sky_models = Some(frame);
        self
    }

    /// Restricts every sky queue to the native portal scissor. None suppresses
    /// sky drawing; world geometry and UI retain their own full-frame scissor.
    #[must_use]
    pub const fn with_sky_window(mut self, window: Option<crate::WorldSkyWindow>) -> Self {
        self.sky_window = window;
        self
    }

    pub(in crate::device) const fn sky_window(self) -> Option<crate::WorldSkyWindow> {
        self.sky_window
    }

    /// Rejects invisible queues before allocating or uploading frame resources.
    pub(super) fn clip_sky(mut self, viewport: crate::WorldScreenWindow) -> Self {
        self.sky_window = self.sky_window.and_then(|window| window.clipped(viewport));
        if self.sky_window.is_none() {
            self.sky_models = None;
            self.sky = None;
            self.celestials = None;
            self.clouds = None;
        }
        self
    }

    /// Supplies 79A870's clear color, chosen from camera fog or black.
    #[must_use]
    pub const fn with_background_color(mut self, color: glam::Vec4) -> Self {
        self.background_color = color;
        self
    }

    pub(in crate::device) const fn background_color(self) -> glam::Vec4 {
        self.background_color
    }

    pub(in crate::device) const fn sky_models(self) -> Option<WorldSkyModelFrame<'a>> {
        self.sky_models
    }

    /// Adds the native sun/moon strips before the additive sky gradient.
    #[must_use]
    pub const fn with_celestials(mut self, frame: crate::WorldCelestialFrame<'a>) -> Self {
        self.celestials = Some(frame);
        self
    }
    pub(in crate::device) const fn celestials(self) -> Option<crate::WorldCelestialFrame<'a>> {
        self.celestials
    }

    /// Adds the native procedural cloud dome after the sky gradient.
    #[must_use]
    pub const fn with_clouds(mut self, frame: WorldCloudFrame<'a>) -> Self {
        self.clouds = Some(frame);
        self
    }

    pub(in crate::device) const fn clouds(self) -> Option<WorldCloudFrame<'a>> {
        self.clouds
    }

    /// Adds the native sky background at the same camera and environment sample.
    #[must_use]
    pub const fn with_sky(mut self, frame: WorldSkyFrame<'a>) -> Self {
        self.sky = Some(frame);
        self
    }

    pub(in crate::device) const fn sky(self) -> Option<WorldSkyFrame<'a>> {
        self.sky
    }

    /// Adds retained liquid draws and the procedural colors sampled for this frame.
    #[must_use]
    pub const fn with_liquids(mut self, frame: LiquidFrame<'a>) -> Self {
        self.liquids = Some(frame);
        self
    }

    pub(in crate::device) const fn liquids(self) -> Option<LiquidFrame<'a>> {
        self.liquids
    }

    /// Adds the circular and directional surface effects after transparent water.
    #[must_use]
    pub const fn with_ripples(mut self, frame: WaterRippleFrame<'a>) -> Self {
        self.ripples = Some(frame);
        self
    }

    pub(in crate::device) const fn ripples(self) -> Option<WaterRippleFrame<'a>> {
        self.ripples
    }

    /// Adds native underwater billboards after all world and M2 effect queues.
    #[must_use]
    pub const fn with_underwater_particles(mut self, frame: UnderwaterParticleFrame<'a>) -> Self {
        self.underwater = Some(frame);
        self
    }

    pub(in crate::device) const fn underwater(self) -> Option<UnderwaterParticleFrame<'a>> {
        self.underwater
    }

    /// Supplies the independent character and pet banks authored by Glue Lua.
    #[must_use]
    pub const fn with_m2_light_banks(
        mut self,
        character: M2SceneUniform,
        pet: M2SceneUniform,
    ) -> Self {
        self.m2[M2SceneLightBank::Character.index()] = character;
        self.m2[M2SceneLightBank::Pet.index()] = pet;
        self
    }

    /// Reserves effect storage from stock's emitter-pool estimates.
    #[must_use]
    pub const fn with_particle_capacity(
        mut self,
        vertex_capacity: usize,
        index_capacity: usize,
    ) -> Self {
        self.particle_vertex_capacity = vertex_capacity;
        self.particle_index_capacity = index_capacity;
        self
    }

    pub(super) const fn terrain(self) -> TerrainSceneUniform {
        self.terrain
    }

    pub(super) const fn world_model(self) -> WorldModelSceneUniform {
        self.world_model
    }

    pub(super) const fn m2(self, light_bank: M2SceneLightBank) -> M2SceneUniform {
        self.m2[light_bank.index()]
    }

    pub(super) const fn particle_vertex_capacity(self) -> usize {
        self.particle_vertex_capacity
    }

    pub(super) const fn particle_index_capacity(self) -> usize {
        self.particle_index_capacity
    }
}

/// One authored sky model's packets and its sampled scene lighting.
/// Bone offsets in these packets start after the ordinary frame's bone palette.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldSkyModelBatch<'a> {
    pub(in crate::device) scene: M2SceneUniform,
    pub(in crate::device) draws: &'a [crate::M2PreparedDraw],
}

impl<'a> WorldSkyModelBatch<'a> {
    /// Retains one skybox's material packets and sampled authored lighting.
    #[must_use]
    pub const fn new(scene: M2SceneUniform, draws: &'a [crate::M2PreparedDraw]) -> Self {
        Self { scene, draws }
    }
}

/// Stars, three ordinary LightSkybox slots, and the independent global override.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldSkyModelFrame<'a> {
    pub(in crate::device) scene: M2SceneUniform,
    pub(in crate::device) bones: &'a [glam::Mat4],
    pub(in crate::device) stars: &'a [crate::M2PreparedDraw],
    pub(in crate::device) skyboxes: [WorldSkyModelBatch<'a>; 4],
}

impl<'a> WorldSkyModelFrame<'a> {
    /// Stars precede celestial strips; authored skyboxes follow the clouds.
    #[must_use]
    pub const fn new(
        scene: M2SceneUniform,
        bones: &'a [glam::Mat4],
        stars: &'a [crate::M2PreparedDraw],
        skyboxes: &'a [crate::M2PreparedDraw],
    ) -> Self {
        Self {
            scene,
            bones,
            stars,
            skyboxes: [
                WorldSkyModelBatch::new(scene, skyboxes),
                WorldSkyModelBatch::new(scene, &[]),
                WorldSkyModelBatch::new(scene, &[]),
                WorldSkyModelBatch::new(scene, &[]),
            ],
        }
    }

    /// Preserves palette slot order and an independent light bank for each model.
    #[must_use]
    pub const fn with_skybox_batches(mut self, batches: [WorldSkyModelBatch<'a>; 3]) -> Self {
        self.skyboxes[0] = batches[0];
        self.skyboxes[1] = batches[1];
        self.skyboxes[2] = batches[2];
        self
    }

    /// Draws the global override after all admitted ordinary skyboxes.
    #[must_use]
    pub const fn with_global_skybox(mut self, batch: WorldSkyModelBatch<'a>) -> Self {
        self.skyboxes[3] = batch;
        self
    }

    /// Returns the total submitted authored-model material batches.
    #[must_use]
    pub const fn draw_count(self) -> usize {
        self.stars.len()
            + self.skyboxes[0].draws.len()
            + self.skyboxes[1].draws.len()
            + self.skyboxes[2].draws.len()
            + self.skyboxes[3].draws.len()
    }

    pub(in crate::device) fn draws(self) -> impl Iterator<Item = &'a crate::M2PreparedDraw> {
        self.stars
            .iter()
            .chain(self.skyboxes.into_iter().flat_map(|batch| batch.draws))
    }
}

/// Draw and bone counts accepted by one unified presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorldFrameReport {
    ripple_draw_count: usize,
    underwater_draw_count: usize,
    sky_draw_count: usize,
    low_detail_draw_count: usize,
    ground_detail_draw_count: usize,
    primary_shadow_draw_count: usize,
    celestial_draw_count: usize,
    sky_model_draw_count: usize,
    terrain_draw_count: usize,
    liquid_draw_count: usize,
    world_model_draw_count: usize,
    m2_draw_count: usize,
    particle_draw_count: usize,
    particle_vertex_count: usize,
    particle_index_count: usize,
    ribbon_draw_count: usize,
    ribbon_vertex_count: usize,
    bone_transform_count: usize,
}

impl WorldFrameReport {
    pub(super) const fn with_primary_shadow_draw_count(mut self, count: usize) -> Self {
        self.primary_shadow_draw_count = count;
        self
    }

    /// Returns the M2 batches rendered into this frame's primary shadow map.
    #[must_use]
    pub const fn primary_shadow_draw_count(self) -> usize {
        self.primary_shadow_draw_count
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) const fn new(
        terrain_draw_count: usize,
        liquid_draw_count: usize,
        world_model_draw_count: usize,
        m2_draw_count: usize,
        particle_draw_count: usize,
        particle_vertex_count: usize,
        particle_index_count: usize,
        ribbon_draw_count: usize,
        ribbon_vertex_count: usize,
        bone_transform_count: usize,
    ) -> Self {
        Self {
            terrain_draw_count,
            ripple_draw_count: 0,
            underwater_draw_count: 0,
            sky_draw_count: 0,
            low_detail_draw_count: 0,
            ground_detail_draw_count: 0,
            primary_shadow_draw_count: 0,
            celestial_draw_count: 0,
            sky_model_draw_count: 0,
            liquid_draw_count,
            world_model_draw_count,
            m2_draw_count,
            particle_draw_count,
            particle_vertex_count,
            particle_index_count,
            ribbon_draw_count,
            ribbon_vertex_count,
            bone_transform_count,
        }
    }

    pub(super) const fn with_low_detail_draw_count(mut self, count: usize) -> Self {
        self.low_detail_draw_count = count;
        self
    }

    /// Returns the number of WDL face banks submitted to the horizon interval.
    #[must_use]
    pub const fn low_detail_draw_count(self) -> usize {
        self.low_detail_draw_count
    }

    /// Records the accepted native detail texture buckets.
    pub(super) const fn with_ground_detail_draw_count(mut self, count: usize) -> Self {
        self.ground_detail_draw_count = count;
        self
    }

    /// Returns the submitted grass and other terrain-detail texture bucket count.
    #[must_use]
    pub const fn ground_detail_draw_count(self) -> usize {
        self.ground_detail_draw_count
    }

    pub(super) const fn with_sky_model_draw_count(mut self, count: usize) -> Self {
        self.sky_model_draw_count = count;
        self
    }
    /// Returns the stars and authored skybox material submission count.
    #[must_use]
    pub const fn sky_model_draw_count(self) -> usize {
        self.sky_model_draw_count
    }

    pub(super) const fn with_celestial_draw_count(mut self, count: usize) -> Self {
        self.celestial_draw_count = count;
        self
    }
    /// Returns the number of native sun/moon strips submitted before the gradient.
    #[must_use]
    pub const fn celestial_draw_count(self) -> usize {
        self.celestial_draw_count
    }

    pub(super) const fn with_sky_draw_count(mut self, count: usize) -> Self {
        self.sky_draw_count = count;
        self
    }

    /// Returns the native sky dome submission count (zero or one).
    #[must_use]
    pub const fn sky_draw_count(self) -> usize {
        self.sky_draw_count
    }

    pub(super) const fn with_ripple_draw_count(mut self, count: usize) -> Self {
        self.ripple_draw_count = count;
        self
    }

    /// Returns the circular/directional ripple passes submitted after transparent water.
    #[must_use]
    pub const fn ripple_draw_count(self) -> usize {
        self.ripple_draw_count
    }

    pub(super) const fn with_underwater_draw_count(mut self, count: usize) -> Self {
        self.underwater_draw_count = count;
        self
    }

    /// Returns the native late-world underwater billboard draw count (zero or one).
    #[must_use]
    pub const fn underwater_draw_count(self) -> usize {
        self.underwater_draw_count
    }

    /// Returns submitted camera-selected MCNK draw count.
    #[must_use]
    pub const fn terrain_draw_count(self) -> usize {
        self.terrain_draw_count
    }

    /// Returns submitted opaque and transparent liquid strip count.
    #[must_use]
    pub const fn liquid_draw_count(self) -> usize {
        self.liquid_draw_count
    }

    /// Returns submitted physical WMO pass count.
    #[must_use]
    pub const fn world_model_draw_count(self) -> usize {
        self.world_model_draw_count
    }

    /// Returns submitted M2 material draw count.
    #[must_use]
    pub const fn m2_draw_count(self) -> usize {
        self.m2_draw_count
    }

    /// Returns submitted placement-local ordinary-particle emitter count.
    #[must_use]
    pub const fn particle_draw_count(self) -> usize {
        self.particle_draw_count
    }

    /// Returns uploaded dynamic PNC0T0 particle vertex count.
    #[must_use]
    pub const fn particle_vertex_count(self) -> usize {
        self.particle_vertex_count
    }

    /// Returns uploaded dynamic UINT32 particle index count.
    #[must_use]
    pub const fn particle_index_count(self) -> usize {
        self.particle_index_count
    }

    /// Returns submitted placement-local M2 ribbon strip count.
    #[must_use]
    pub const fn ribbon_draw_count(self) -> usize {
        self.ribbon_draw_count
    }

    /// Returns uploaded dynamic PCT0 ribbon vertex count.
    #[must_use]
    pub const fn ribbon_vertex_count(self) -> usize {
        self.ribbon_vertex_count
    }

    /// Returns uploaded M2 bone-transform count.
    #[must_use]
    pub const fn bone_transform_count(self) -> usize {
        self.bone_transform_count
    }
}
