//! Fence-retired glare vertex banks and two nonblocking visibility queries.

#![allow(unsafe_code)]

use crate::device::vulkan_celestial::CelestialFrameResources;
use crate::device::vulkan_texture::BlpTextureRegistry;
use crate::{VulkanError, WorldCelestialDraw};
use ash::{Device, vk};

/// A prepared bank needs only its transform; geometry lives in the slot buffer.
#[derive(Clone, Copy)]
struct Draw {
    matrix: [f32; 16],
}

/// Four fixed banks: one probe and one visible billboard for each native owner.
pub(in crate::device::vulkan_world_frame) struct GlareSlot {
    pool: vk::QueryPool,
    banks: [CelestialFrameResources; 4],
    draws: [Option<Draw>; 4],
    pending: [bool; 2],
}

impl Default for GlareSlot {
    fn default() -> Self {
        Self {
            pool: vk::QueryPool::null(),
            banks: [const { CelestialFrameResources::empty() }; 4],
            draws: [None; 4],
            pending: [false; 2],
        }
    }
}

impl GlareSlot {
    /// Allocates only when the scene first supplies glare resources.
    pub(super) fn ensure(&mut self, device: &Device) -> Result<(), VulkanError> {
        if self.pool == vk::QueryPool::null() {
            let info = vk::QueryPoolCreateInfo::default()
                .query_type(vk::QueryType::OCCLUSION)
                .query_count(2);
            // SAFETY: Two ordinary occlusion queries need no extension or external payload.
            self.pool = unsafe { device.create_query_pool(&info, None) }.map_err(|source| {
                VulkanError::operation("create glare visibility queries", source)
            })?;
        }
        Ok(())
    }

    /// Reads only queries from this retired slot's successful submission.
    pub(super) fn collect(&mut self, device: &Device) -> Result<[Option<u64>; 2], VulkanError> {
        let mut result = [None; 2];
        for (index, pending) in std::mem::take(&mut self.pending).into_iter().enumerate() {
            if !pending {
                continue;
            }
            let mut samples = [0_u64];
            // SAFETY: The normal frame fence precedes this read; queried indices
            // were reset, begun and ended by the corresponding submission.
            match unsafe {
                device.get_query_pool_results(
                    self.pool,
                    index as u32,
                    &mut samples,
                    vk::QueryResultFlags::TYPE_64,
                )
            } {
                Ok(()) => result[index] = Some(samples[0]),
                Err(vk::Result::NOT_READY) => {}
                Err(source) => return Err(VulkanError::operation("read glare visibility", source)),
            }
        }
        Ok(result)
    }

    /// Frame omission clears draws without resetting the process fade or cached count.
    pub(super) fn clear(&mut self) {
        self.draws = [None; 4];
    }

    /// Uploads the uncut celestial disc with a color-masked pipeline layout.
    #[allow(clippy::too_many_arguments)] // One fixed bank binds independent resource owners.
    pub(super) fn write_probe(
        &mut self,
        index: usize,
        device: &Device,
        allocator: &vk_mem::Allocator,
        textures: &BlpTextureRegistry,
        layout: vk::DescriptorSetLayout,
        draw: WorldCelestialDraw<'_>,
    ) -> Result<(), VulkanError> {
        self.write(index * 2, device, allocator, textures, layout, draw)
    }

    /// Uploads the larger visible glare into a distinct bank from its probe.
    #[allow(clippy::too_many_arguments)] // One fixed bank binds independent resource owners.
    pub(super) fn write_visible(
        &mut self,
        index: usize,
        device: &Device,
        allocator: &vk_mem::Allocator,
        textures: &BlpTextureRegistry,
        layout: vk::DescriptorSetLayout,
        draw: WorldCelestialDraw<'_>,
    ) -> Result<(), VulkanError> {
        self.write(index * 2 + 1, device, allocator, textures, layout, draw)
    }

    /// Writes only after slot retirement, preserving independent descriptors.
    #[allow(clippy::too_many_arguments)] // Backend ownership mirrors the existing celestial bank API.
    fn write(
        &mut self,
        index: usize,
        device: &Device,
        allocator: &vk_mem::Allocator,
        textures: &BlpTextureRegistry,
        layout: vk::DescriptorSetLayout,
        draw: WorldCelestialDraw<'_>,
    ) -> Result<(), VulkanError> {
        self.banks[index].ensure(device, allocator, layout)?;
        self.banks[index].write(device, allocator, textures, draw)?;
        self.draws[index] = Some(Draw {
            matrix: draw.view_projection().to_cols_array(),
        });
        Ok(())
    }

    /// Query resets must occur outside the active dynamic-rendering scope.
    pub(in crate::device::vulkan_world_frame) fn reset(
        &self,
        device: &Device,
        command: vk::CommandBuffer,
    ) {
        if self.pool != vk::QueryPool::null() {
            // SAFETY: The slot fence excludes earlier use; both indices are owned here.
            unsafe { device.cmd_reset_query_pool(command, self.pool, 0, 2) };
        }
    }

    /// Restores the full depth interval; shaders place probes in the reserved sky range.
    pub(super) fn record(
        &self,
        device: &Device,
        command: vk::CommandBuffer,
        viewport: vk::Viewport,
        probe: (vk::Pipeline, vk::PipelineLayout),
        visible: (vk::Pipeline, vk::PipelineLayout),
    ) {
        if self.draws.iter().all(Option::is_none) {
            return;
        }
        // SAFETY: Both PCT variants declare dynamic viewports and share the world window.
        unsafe {
            device.cmd_set_viewport(
                command,
                0,
                &[vk::Viewport {
                    min_depth: 0.,
                    max_depth: 1.,
                    ..viewport
                }],
            )
        };
        for index in 0..2 {
            if let Some(draw) = self.draws[index * 2] {
                // SAFETY: Reset occurs before rendering; no query is active and precise
                // occlusion was explicitly enabled on this logical device.
                unsafe {
                    device.cmd_begin_query(
                        command,
                        self.pool,
                        index as u32,
                        vk::QueryControlFlags::PRECISE,
                    )
                };
                record_draw(device, command, &self.banks[index * 2], draw, probe);
                // SAFETY: Matches the begin for this owner in this rendering scope.
                unsafe { device.cmd_end_query(command, self.pool, index as u32) };
            }
            if let Some(draw) = self.draws[index * 2 + 1] {
                record_draw(device, command, &self.banks[index * 2 + 1], draw, visible);
            }
        }
    }

    /// Marks queries readable only after the queue accepted this frame.
    pub(in crate::device::vulkan_world_frame) fn submitted(&mut self) {
        self.pending = [self.draws[0].is_some(), self.draws[2].is_some()];
    }

    /// Releases every query and packed bank after the owner's normal fence.
    pub(in crate::device::vulkan_world_frame) fn destroy(
        &mut self,
        device: &Device,
        allocator: &vk_mem::Allocator,
    ) {
        for bank in &mut self.banks {
            bank.destroy(device, allocator);
        }
        if self.pool != vk::QueryPool::null() {
            // SAFETY: No submitted command still references this retired pool.
            unsafe { device.destroy_query_pool(self.pool, None) };
        }
        self.pool = vk::QueryPool::null();
        self.pending = [false; 2];
        self.clear();
    }
}

/// The glare is the final world geometry, so no later draw relies on cached bindings.
fn record_draw(
    device: &Device,
    command: vk::CommandBuffer,
    bank: &CelestialFrameResources,
    draw: Draw,
    pipeline: (vk::Pipeline, vk::PipelineLayout),
) {
    let (vertex, offset) = bank.vertex_buffer();
    let (index, index_offset) = bank.index_buffer();
    let mut pushes = [0_u8; 64];
    for (word, value) in pushes.as_chunks_mut::<4>().0.iter_mut().zip(draw.matrix) {
        *word = value.to_le_bytes();
    }
    // SAFETY: Retired-slot preparation populated a four-index strip and matching
    // descriptor; both PCT layouts accept this 64-byte matrix and vertex ABI.
    unsafe {
        device.cmd_bind_pipeline(command, vk::PipelineBindPoint::GRAPHICS, pipeline.0);
        device.cmd_bind_vertex_buffers(command, 0, &[vertex], &[offset]);
        device.cmd_bind_index_buffer(command, index, index_offset, vk::IndexType::UINT16);
        device.cmd_bind_descriptor_sets(
            command,
            vk::PipelineBindPoint::GRAPHICS,
            pipeline.1,
            0,
            &[bank.descriptor()],
            &[],
        );
        device.cmd_push_constants(
            command,
            pipeline.1,
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            0,
            &pushes,
        );
        device.cmd_draw_indexed(command, 4, 1, 0, 0, 0);
    }
}
