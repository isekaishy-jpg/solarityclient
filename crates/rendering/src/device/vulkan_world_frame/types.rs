//! Public scene snapshot and observable unified-world submission facts.

use crate::{
    LiquidFrame, M2SceneLightBank, M2SceneUniform, TerrainSceneUniform, WaterRippleFrame,
    WorldModelSceneUniform,
};

/// One coherent terrain, WMO, M2, and M2-effect scene snapshot for a world frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldFrameScene<'a> {
    terrain: TerrainSceneUniform,
    world_model: WorldModelSceneUniform,
    m2: [M2SceneUniform; M2SceneLightBank::COUNT],
    particle_vertex_capacity: usize,
    particle_index_capacity: usize,
    liquids: Option<LiquidFrame<'a>>,
    ripples: Option<WaterRippleFrame<'a>>,
}

impl<'a> WorldFrameScene<'a> {
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
            particle_vertex_capacity: 0,
            particle_index_capacity: 0,
            liquids: None,
            ripples: None,
        }
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

/// Draw and bone counts accepted by one unified presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorldFrameReport {
    ripple_draw_count: usize,
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

    pub(super) const fn with_ripple_draw_count(mut self, count: usize) -> Self {
        self.ripple_draw_count = count;
        self
    }

    /// Returns the circular/directional ripple passes submitted after transparent water.
    #[must_use]
    pub const fn ripple_draw_count(self) -> usize {
        self.ripple_draw_count
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
