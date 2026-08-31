//! Public scene snapshot and observable unified-world submission facts.

use crate::{M2SceneUniform, TerrainSceneUniform, WorldModelSceneUniform};

/// One coherent terrain, WMO, M2, and M2-effect scene snapshot for a world frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldFrameScene {
    terrain: TerrainSceneUniform,
    world_model: WorldModelSceneUniform,
    m2: M2SceneUniform,
}

impl WorldFrameScene {
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
            m2,
        }
    }

    pub(super) const fn terrain(self) -> TerrainSceneUniform {
        self.terrain
    }

    pub(super) const fn world_model(self) -> WorldModelSceneUniform {
        self.world_model
    }

    pub(super) const fn m2(self) -> M2SceneUniform {
        self.m2
    }
}

/// Draw and bone counts accepted by one unified presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorldFrameReport {
    terrain_draw_count: usize,
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

    /// Returns submitted camera-selected MCNK draw count.
    #[must_use]
    pub const fn terrain_draw_count(self) -> usize {
        self.terrain_draw_count
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
