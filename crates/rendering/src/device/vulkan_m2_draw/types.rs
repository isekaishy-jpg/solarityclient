//! Immutable draw data safe to consume during Vulkan command recording.

use crate::device::{M2MeshHandle, M2PipelineHandle, M2TextureSetHandle};
use crate::model::{M2DrawPushConstants, M2MaterialUniform, M2ShadowMaterial};

use super::M2SceneLightBank;

/// One fully validated M2 indexed draw and its per-draw GPU state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2PreparedDraw {
    mesh: M2MeshHandle,
    pipeline: M2PipelineHandle,
    texture_set: M2TextureSetHandle,
    first_index: u32,
    index_count: u32,
    material: M2MaterialUniform,
    push_constants: M2DrawPushConstants,
    required_bone_transforms: usize,
    light_bank: M2SceneLightBank,
    scene_index: Option<u32>,
    priority_plane: i16,
    effect_interleave: bool,
    scene_order: u32,
    shadow_material: Option<M2ShadowMaterial>,
}

impl M2PreparedDraw {
    /// Constructs a packet after the renderer validates every resource join.
    #[allow(clippy::too_many_arguments)]
    pub(super) const fn new(
        mesh: M2MeshHandle,
        pipeline: M2PipelineHandle,
        texture_set: M2TextureSetHandle,
        first_index: u32,
        index_count: u32,
        material: M2MaterialUniform,
        push_constants: M2DrawPushConstants,
        required_bone_transforms: usize,
        priority_plane: i16,
        effect_interleave: bool,
        shadow_material: Option<M2ShadowMaterial>,
    ) -> Self {
        Self {
            mesh,
            pipeline,
            texture_set,
            first_index,
            index_count,
            material,
            push_constants,
            required_bone_transforms,
            light_bank: M2SceneLightBank::Environment,
            scene_index: None,
            priority_plane,
            effect_interleave,
            scene_order: u32::MAX,
            shadow_material,
        }
    }

    /// Selects the world instance scene supplied in `WorldFrameScene`.
    #[must_use]
    pub const fn with_scene_index(mut self, index: Option<u32>) -> Self {
        self.scene_index = index;
        self
    }

    /// Returns the world instance scene, or the fixed Glue/sky bank.
    #[must_use]
    pub const fn scene_index(self) -> Option<u32> {
        self.scene_index
    }

    /// Assigns the stock Glue light bank used by unified-frame presentation.
    #[must_use]
    pub const fn with_light_bank(mut self, light_bank: M2SceneLightBank) -> Self {
        self.light_bank = light_bank;
        self
    }

    /// Returns the device-local vertex/index allocation identity.
    #[must_use]
    pub const fn mesh(self) -> M2MeshHandle {
        self.mesh
    }

    /// Returns the exact compiled effect/permutation identity.
    #[must_use]
    pub const fn pipeline(self) -> M2PipelineHandle {
        self.pipeline
    }

    /// Returns the persistent sampled-texture descriptor identity.
    #[must_use]
    pub const fn texture_set(self) -> M2TextureSetHandle {
        self.texture_set
    }

    /// Returns the first unsigned-short index in the uploaded profile.
    #[must_use]
    pub const fn first_index(self) -> u32 {
        self.first_index
    }

    /// Returns the number of indices submitted for this material batch.
    #[must_use]
    pub const fn index_count(self) -> u32 {
        self.index_count
    }

    /// Returns the exact std140 per-draw material block.
    #[must_use]
    pub const fn material(self) -> M2MaterialUniform {
        self.material
    }

    /// Returns the original caster-material queue, before instance spatial admission.
    #[must_use]
    pub const fn shadow_material(self) -> Option<M2ShadowMaterial> {
        self.shadow_material
    }

    /// Returns the exact 16-byte per-draw push block.
    #[must_use]
    pub const fn push_constants(self) -> M2DrawPushConstants {
        self.push_constants
    }

    /// Returns the minimum global transform count needed by this mesh instance.
    #[must_use]
    pub const fn required_bone_transforms(self) -> usize {
        self.required_bone_transforms
    }

    /// Returns the scene-light bank selected for this model instance.
    #[must_use]
    pub const fn light_bank(self) -> M2SceneLightBank {
        self.light_bank
    }

    /// Returns the authored plane used to insert ordinary effects.
    #[must_use]
    pub const fn priority_plane(self) -> i16 {
        self.priority_plane
    }

    /// Reports whether effects through this plane precede this mesh draw.
    #[must_use]
    pub const fn effect_interleave(self) -> bool {
        self.effect_interleave
    }

    /// Assigns this packet's position in the unified stock scene-element queue.
    #[must_use]
    pub const fn with_scene_order(mut self, scene_order: u32) -> Self {
        self.scene_order = scene_order;
        self
    }

    /// Returns this packet's position in the unified scene-element queue.
    #[must_use]
    pub const fn scene_order(self) -> u32 {
        self.scene_order
    }
}
