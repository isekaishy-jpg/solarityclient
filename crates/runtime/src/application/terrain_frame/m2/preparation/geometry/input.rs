//! Immutable draw demands distinguish visible simulation from independent shadow work.

use super::super::super::shadow::ShadowInput;
use glam::{Mat4, Vec3, Vec4};
use solarity_rendering::{
    M2AnimationClock, M2LiquidPasses, M2ParticleColorReplacement, M2SceneLightBank,
};

/// One model's draw preparation follows ordered callbacks, including offscreen casters.
#[derive(Clone, Copy)]
pub(in super::super::super) struct GeometryInput {
    pub placement_index: usize,
    pub trace: solarity_profiling::TraceContext,
    pub source_index: usize,
    pub clock: M2AnimationClock,
    pub transform: Mat4,
    pub model_view: Mat4,
    pub shadow: ShadowInput,
    pub primary_shadow: bool,
    pub environment_maps: u8,
    pub visible: Option<VisibleGeometryInput>,
}

/// Only camera-admitted models advance effect time, particles and ribbons.
#[derive(Clone, Copy)]
pub(in super::super::super) struct VisibleGeometryInput {
    pub effect_delta_seconds: f32,
    pub instance_color: Vec4,
    pub placement_fog_color: Vec3,
    pub light_bank: M2SceneLightBank,
    pub scene_index: Option<u32>,
    pub effect_retiring: bool,
    pub particle_liquid: M2LiquidPasses,
    pub model_liquid: M2LiquidPasses,
    pub instance_distance: f32,
    pub instance_identity: usize,
    pub model_distance_sort: bool,
    pub particle_colors: Option<M2ParticleColorReplacement>,
}
