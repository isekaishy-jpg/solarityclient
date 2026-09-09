//! One immutable geometry upload and descriptor bank per retained detail chunk.

#![allow(unsafe_code)]

use std::sync::Arc;

use ash::{Device, vk};
use vk_mem::Alloc;

use crate::VulkanError;
use crate::device::vulkan_texture::BlpTextureRegistry;

use super::{GroundDetailDraw, GroundDetailFrame};

/// Live GPU dependencies and device limits needed for an unsubmitted detail chunk.
pub(in crate::device) struct DetailCreateContext<'a> {
    pub(in crate::device) device: &'a Device,
    pub(in crate::device) allocator: &'a vk_mem::Allocator,
    pub(in crate::device) textures: &'a BlpTextureRegistry,
    pub(in crate::device) descriptor: vk::DescriptorSetLayout,
    pub(in crate::device) anisotropy_supported: bool,
    pub(in crate::device) maximum_anisotropy: f32,
}

/// The CPU plan is also the lifetime token held by runtime and submitted slots.
pub(in crate::device) struct DetailGpuMesh {
    draw: GroundDetailDraw,
    buffer: vk::Buffer,
    allocation: Option<vk_mem::Allocation>,
    sampler: vk::Sampler,
    pool: vk::DescriptorPool,
    sets: Vec<vk::DescriptorSet>,
}

impl DetailGpuMesh {
    /// Publishes a complete immutable resource, cleaning all partial allocations.
    fn create(
        context: &DetailCreateContext<'_>,
        draw: &GroundDetailDraw,
    ) -> Result<Self, VulkanError> {
        let mut result = Self {
            draw: draw.clone(),
            buffer: vk::Buffer::null(),
            allocation: None,
            sampler: vk::Sampler::null(),
            pool: vk::DescriptorPool::null(),
            sets: Vec::new(),
        };
        if let Err(error) = result.initialize(context) {
            result.destroy(context.device, context.allocator);
            return Err(error);
        }
        Ok(result)
    }

    /// Uploads only newly encountered geometry; unchanged frames retain this bank.
    fn initialize(&mut self, context: &DetailCreateContext<'_>) -> Result<(), VulkanError> {
        let bytes = self.draw.plan().bytes();
        let info = vk::BufferCreateInfo::default()
            .size(bytes.len() as u64)
            .usage(vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::INDEX_BUFFER)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let allocation_info = vk_mem::AllocationCreateInfo {
            flags: vk_mem::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
            usage: vk_mem::MemoryUsage::AutoPreferDevice,
            required_flags: vk::MemoryPropertyFlags::HOST_VISIBLE,
            ..Default::default()
        };
        // SAFETY: The serialized immutable plan supplies the complete buffer extent.
        let (buffer, allocation) =
            unsafe { context.allocator.create_buffer(&info, &allocation_info) }
                .map_err(|source| VulkanError::operation("create detail buffer", source))?;
        self.buffer = buffer;
        self.allocation = Some(allocation);
        let allocation = self
            .allocation
            .as_mut()
            .ok_or(VulkanError::WorldFrameCapacity)?;
        // SAFETY: The allocation is new and has no queued readers.
        let destination = unsafe { context.allocator.map_memory(allocation) }
            .map_err(|source| VulkanError::operation("map detail buffer", source))?;
        // SAFETY: The destination allocation has exactly bytes.len() writable bytes.
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), destination, bytes.len());
        }
        let flushed = context
            .allocator
            .flush_allocation(allocation, 0, bytes.len() as u64)
            .map_err(|source| VulkanError::operation("flush detail buffer", source));
        // SAFETY: Every successful mapping is released, including flush failures.
        unsafe {
            context.allocator.unmap_memory(allocation);
        }
        flushed?;
        let requested = self.draw.filtering().requested_anisotropy();
        let anisotropy = if context.anisotropy_supported {
            requested.min(context.maximum_anisotropy).max(1.0)
        } else {
            1.0
        };
        // 7D9990 calls 681BE0 with both wrapping bits enabled for detail textures.
        let info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(if self.draw.filtering().uses_linear_mips() {
                vk::SamplerMipmapMode::LINEAR
            } else {
                vk::SamplerMipmapMode::NEAREST
            })
            .address_mode_u(vk::SamplerAddressMode::REPEAT)
            .address_mode_v(vk::SamplerAddressMode::REPEAT)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .min_lod(self.draw.base_mip().level())
            .max_lod(vk::LOD_CLAMP_NONE)
            .anisotropy_enable(anisotropy > 1.0)
            .max_anisotropy(anisotropy);
        // SAFETY: Anisotropy is enabled only with the supported feature and capped limit.
        self.sampler = unsafe { context.device.create_sampler(&info, None) }
            .map_err(|source| VulkanError::operation("create detail sampler", source))?;
        let count = self.draw.textures().len() as u32;
        let sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: count,
        }];
        let info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(count)
            .pool_sizes(&sizes);
        // SAFETY: Each nonempty batch needs exactly one immutable texture descriptor.
        self.pool = unsafe { context.device.create_descriptor_pool(&info, None) }
            .map_err(|source| VulkanError::operation("create detail descriptor pool", source))?;
        let layouts = vec![context.descriptor; count as usize];
        let info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(self.pool)
            .set_layouts(&layouts);
        // SAFETY: The prepared detail pipeline owns the compatible live descriptor layout.
        self.sets = unsafe { context.device.allocate_descriptor_sets(&info) }
            .map_err(|source| VulkanError::operation("allocate detail texture sets", source))?;
        for (texture, set) in self.draw.textures().iter().zip(&self.sets) {
            let view = context
                .textures
                .view(*texture)
                .ok_or(VulkanError::UnknownBlpTextureHandle)?;
            let images = [vk::DescriptorImageInfo::default()
                .sampler(self.sampler)
                .image_view(view)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let writes = [vk::WriteDescriptorSet::default()
                .dst_set(*set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&images)];
            // SAFETY: All image views are resident and no descriptor has been submitted.
            unsafe {
                context.device.update_descriptor_sets(&writes, &[]);
            }
        }
        Ok(())
    }

    pub(in crate::device) const fn vertex_buffer(&self) -> (vk::Buffer, u64) {
        (self.buffer, 0)
    }
    pub(in crate::device) fn index_buffer(&self) -> (vk::Buffer, u64) {
        (self.buffer, (self.draw.plan().vertices().len() * 36) as u64)
    }
    pub(in crate::device) fn sets(&self) -> &[vk::DescriptorSet] {
        &self.sets
    }

    /// Releases buffers only after both runtime ownership and all slot pins end.
    fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        // SAFETY: Retirement proves no GPU readers; failed creation never submitted.
        unsafe {
            if self.pool != vk::DescriptorPool::null() {
                device.destroy_descriptor_pool(self.pool, None);
            }
            if self.sampler != vk::Sampler::null() {
                device.destroy_sampler(self.sampler, None);
            }
            if let Some(mut allocation) = self.allocation.take() {
                allocator.destroy_buffer(self.buffer, &mut allocation);
            }
        }
        self.buffer = vk::Buffer::null();
        self.pool = vk::DescriptorPool::null();
        self.sampler = vk::Sampler::null();
    }
}

/// Reuses visible detail generations and retires them without a device-idle wait.
#[derive(Default)]
pub(in crate::device) struct DetailRegistry {
    meshes: Vec<DetailGpuMesh>,
}

impl DetailRegistry {
    /// Collects abandoned generations before uploading any newly visible chunks.
    pub(in crate::device) fn ensure(
        &mut self,
        context: &DetailCreateContext<'_>,
        frame: Option<GroundDetailFrame<'_>>,
    ) -> Result<(), VulkanError> {
        // Filtering changes can retain multiple descriptor variants for one plan.
        // Count all registry pins so variants cannot keep each other alive forever.
        let mut owned = std::collections::HashMap::new();
        for mesh in &self.meshes {
            *owned.entry(Arc::as_ptr(mesh.draw.plan())).or_insert(0) += 1;
        }
        self.meshes.retain_mut(|mesh| {
            if Arc::strong_count(mesh.draw.plan()) > owned[&Arc::as_ptr(mesh.draw.plan())] {
                return true;
            }
            mesh.destroy(context.device, context.allocator);
            false
        });
        if let Some(frame) = frame {
            for draw in frame
                .draws()
                .iter()
                .filter(|draw| !draw.plan().indices().is_empty())
            {
                if self.get(draw).is_none() {
                    self.meshes.push(DetailGpuMesh::create(context, draw)?);
                }
            }
        }
        Ok(())
    }

    pub(in crate::device) fn get(&self, draw: &GroundDetailDraw) -> Option<&DetailGpuMesh> {
        self.meshes.iter().find(|mesh| mesh.draw == *draw)
    }

    /// Renderer shutdown has retired every world slot before releasing this registry.
    pub(in crate::device) fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        for mesh in &mut self.meshes {
            mesh.destroy(device, allocator);
        }
        self.meshes.clear();
    }
}
