//! Immutable detail geometry with shared, fence-retained texture bindings.

#![allow(unsafe_code)]

use std::sync::Arc;

use ash::{Device, vk};
use vk_mem::Alloc;

use crate::VulkanError;
use crate::device::vulkan_texture::BlpTextureRegistry;

use super::material::{DetailMaterial, DetailMaterialKey, DetailMaterialRegistry};
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
    materials: Vec<Arc<DetailMaterial>>,
}

impl DetailGpuMesh {
    /// Publishes a complete immutable resource, cleaning all partial allocations.
    fn create(
        context: &DetailCreateContext<'_>,
        draw: &GroundDetailDraw,
        materials: &mut DetailMaterialRegistry,
    ) -> Result<Self, VulkanError> {
        let mut result = Self {
            draw: draw.clone(),
            buffer: vk::Buffer::null(),
            allocation: None,
            materials: Vec::new(),
        };
        if let Err(error) = result.initialize(context, materials) {
            result.destroy(context.device, context.allocator);
            materials.retire_unused(context.device);
            return Err(error);
        }
        Ok(result)
    }

    /// Uploads only newly encountered geometry; unchanged frames retain this bank.
    fn initialize(
        &mut self,
        context: &DetailCreateContext<'_>,
        materials: &mut DetailMaterialRegistry,
    ) -> Result<(), VulkanError> {
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
        for &texture in self.draw.textures() {
            self.materials.push(materials.acquire(
                context,
                DetailMaterialKey {
                    texture,
                    filtering: self.draw.filtering(),
                    base_mip: self.draw.base_mip(),
                },
            )?);
        }
        Ok(())
    }

    pub(in crate::device) const fn vertex_buffer(&self) -> (vk::Buffer, u64) {
        (self.buffer, 0)
    }
    pub(in crate::device) fn index_buffer(&self) -> (vk::Buffer, u64) {
        (self.buffer, (self.draw.plan().vertices().len() * 36) as u64)
    }
    pub(in crate::device) fn sets(&self) -> impl Iterator<Item = vk::DescriptorSet> {
        self.materials.iter().map(|material| material.set())
    }

    /// Releases buffers only after both runtime ownership and all slot pins end.
    fn destroy(&mut self, _device: &Device, allocator: &vk_mem::Allocator) {
        // SAFETY: Retirement proves no GPU readers; failed creation never submitted.
        unsafe {
            if let Some(mut allocation) = self.allocation.take() {
                allocator.destroy_buffer(self.buffer, &mut allocation);
            }
        }
        self.buffer = vk::Buffer::null();
        self.materials.clear();
    }
}

/// Reuses visible detail generations and retires them without a device-idle wait.
#[derive(Default)]
pub(in crate::device) struct DetailRegistry {
    meshes: Vec<DetailGpuMesh>,
    materials: DetailMaterialRegistry,
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
                    self.meshes
                        .push(DetailGpuMesh::create(context, draw, &mut self.materials)?);
                }
            }
        }
        // Arriving meshes can reuse banks whose preceding geometry just left.
        self.materials.retire_unused(context.device);
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
        self.materials.destroy(device);
    }
}
