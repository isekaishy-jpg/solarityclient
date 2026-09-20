//! Synchronous writes from borrowed frame pages into an available GPU slot.

#![allow(unsafe_code)]

use super::{WorldFrameSlot, copy_bytes, indexed_offset};
use crate::device::VulkanError;
use crate::device::vulkan_m2_draw::{M2PreparedDraw, M2SceneLightBank};
use crate::device::vulkan_world_model_draw::WorldModelPreparedDraw;
use crate::{M2ParticleRenderVertex, M2RibbonRenderVertex, WorldFrameScene};

impl WorldFrameSlot {
    /// Copies borrowed frame streams only after the slot fence has completed.
    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn write(
        &mut self,
        allocator: &vk_mem::Allocator,
        scene: WorldFrameScene<'_>,
        bone_transforms: &(impl crate::M2BonePaletteSource + ?Sized),
        world_model_draws: &[WorldModelPreparedDraw],
        m2_draws: &[M2PreparedDraw],
        particle_vertices: &[M2ParticleRenderVertex],
        particle_indices: &[u32],
        ribbon_vertices: &[M2RibbonRenderVertex],
    ) -> Result<(), VulkanError> {
        let allocation = self.buffer_allocation.as_ref().ok_or_else(|| {
            VulkanError::operation("access world frame buffer", "allocation is unavailable")
        })?;
        let destination = allocator
            .get_allocation_info(allocation)
            .mapped_data
            .cast::<u8>();
        if destination.is_null() {
            return Err(VulkanError::operation(
                "access world frame mapping",
                "persistent mapping is unavailable",
            ));
        }
        // SAFETY: VMA reports a persistent mapping covering `total_bytes`, and
        // the slot fence was waited before this CPU write.
        (|| {
            copy_bytes(
                destination,
                self.layout.terrain_scene_offset,
                &scene.terrain().to_bytes(),
                self.layout.total_bytes,
            )?;
            copy_bytes(
                destination,
                self.layout.world_model_scene_offset,
                &scene.world_model().to_bytes(),
                self.layout.total_bytes,
            )?;
            for light_bank in [
                M2SceneLightBank::Environment,
                M2SceneLightBank::Character,
                M2SceneLightBank::Pet,
            ] {
                let mut m2_scene = scene.m2(light_bank);
                if let Some(frame) = scene.primary_shadows() {
                    m2_scene =
                        m2_scene.with_world_shadow(frame.projection(), scene.environment_shadows());
                }
                copy_bytes(
                    destination,
                    indexed_offset(
                        self.layout.m2_scene_offset,
                        self.layout.m2_scene_stride,
                        light_bank.index(),
                    )?,
                    &m2_scene.to_bytes(),
                    self.layout.total_bytes,
                )?;
            }
            for (index, instance) in scene.m2_instance_scenes().iter().enumerate() {
                let mut instance = *instance;
                if let Some(frame) = scene.primary_shadows() {
                    instance =
                        instance.with_world_shadow(frame.projection(), scene.environment_shadows());
                }
                copy_bytes(
                    destination,
                    indexed_offset(
                        self.layout.m2_scene_offset,
                        self.layout.m2_scene_stride,
                        index + 8,
                    )?,
                    &instance.to_bytes(),
                    self.layout.total_bytes,
                )?;
            }
            let sky_models = scene.sky_models();
            let sky_bones = sky_models.map_or(&[][..], |frame| frame.bones);
            copy_bytes(
                destination,
                self.layout.m2_scene_offset + self.layout.m2_scene_stride * 3,
                &sky_models
                    .map_or_else(
                        || scene.m2(M2SceneLightBank::Environment),
                        |frame| frame.scene,
                    )
                    .to_bytes(),
                self.layout.total_bytes,
            )?;
            for slot in 0..4 {
                copy_bytes(
                    destination,
                    self.layout.m2_scene_offset + self.layout.m2_scene_stride * (4 + slot as u64),
                    &sky_models
                        .map_or_else(
                            || scene.m2(M2SceneLightBank::Environment),
                            |frame| frame.skyboxes[slot].scene,
                        )
                        .to_bytes(),
                    self.layout.total_bytes,
                )?;
            }
            let bone_offset = usize::try_from(self.layout.bone_offset)
                .map_err(|_| VulkanError::WorldFrameCapacity)?;
            let bone_bytes = usize::try_from(self.layout.bone_bytes)
                .map_err(|_| VulkanError::WorldFrameCapacity)?;
            if bone_bytes > isize::MAX as usize
                || self
                    .layout
                    .bone_offset
                    .checked_add(self.layout.bone_bytes)
                    .is_none_or(|end| end > self.layout.total_bytes)
            {
                return Err(VulkanError::WorldFrameCapacity);
            }
            // SAFETY: This exclusive slot has a persistent mapping of total_bytes;
            // the validated layout gives a disjoint bone range within that mapping.
            let bones =
                unsafe { std::slice::from_raw_parts_mut(destination.add(bone_offset), bone_bytes) };
            crate::model::write_palette_bytes(bone_transforms, sky_bones, bones)?;
            for (index, material) in world_model_draws
                .iter()
                .map(|draw| draw.material())
                .chain(scene.environment_shadows().into_iter().flat_map(|frame| {
                    frame
                        .wmo_casters()
                        .iter()
                        .map(|caster| caster.draw.material())
                }))
                .enumerate()
            {
                copy_bytes(
                    destination,
                    indexed_offset(
                        self.layout.world_model_material_offset,
                        self.layout.world_model_material_stride,
                        index,
                    )?,
                    &material.to_bytes(scene.world_model().view()),
                    self.layout.total_bytes,
                )?;
            }
            for (index, draw) in m2_draws
                .iter()
                .chain(sky_models.into_iter().flat_map(|frame| frame.draws()))
                .chain(
                    scene
                        .primary_shadows()
                        .into_iter()
                        .flat_map(|frame| frame.casters()),
                )
                .chain(
                    scene
                        .environment_shadows()
                        .into_iter()
                        .flat_map(|frame| frame.m2_casters().iter().map(|caster| &caster.draw)),
                )
                .copied()
                .enumerate()
            {
                copy_bytes(
                    destination,
                    indexed_offset(
                        self.layout.m2_material_offset,
                        self.layout.m2_material_stride,
                        index,
                    )?,
                    &draw.instance_bytes(),
                    self.layout.total_bytes,
                )?;
            }
            super::effects::write_stream(
                destination,
                self.layout.particle_vertex_offset,
                self.layout.total_bytes,
                particle_vertices,
                M2ParticleRenderVertex::to_bytes,
            )?;
            super::effects::write_stream(
                destination,
                self.layout.particle_index_offset,
                self.layout.total_bytes,
                particle_indices,
                u32::to_le_bytes,
            )?;
            super::effects::write_stream(
                destination,
                self.layout.ribbon_vertex_offset,
                self.layout.total_bytes,
                ribbon_vertices,
                M2RibbonRenderVertex::to_bytes,
            )?;
            allocator
                .flush_allocation(allocation, 0, self.layout.total_bytes)
                .map_err(|source| VulkanError::operation("flush world frame buffer", source))
        })()
    }
}
