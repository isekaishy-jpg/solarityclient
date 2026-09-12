//! Fence-local liquid uniforms, sampled descriptors, and procedural texture uploads.

#![allow(unsafe_code)]

use ash::{Device, vk};
use vk_mem::Alloc;

use crate::device::capacity::geometric_capacity;
use crate::device::vulkan_texture::BlpTextureRegistry;
use crate::device::{VulkanError, WorldModelTextureFiltering};
use crate::{LiquidDepthTexture, LiquidShaderUniform};

use super::LiquidFrame;
use super::draw::depth_index;
use super::frame_image::DepthImage;

/// Retired-slot allocation inputs and enabled adapter filtering limits.
pub(in crate::device) struct LiquidFrameCreateContext<'a> {
    pub device: &'a Device,
    pub allocator: &'a vk_mem::Allocator,
    pub layouts: [vk::DescriptorSetLayout; 2],
    pub count: usize,
    pub alignment: u64,
    pub filtering: WorldModelTextureFiltering,
    /// One when the logical device has no enabled anisotropy feature.
    pub maximum_anisotropy: f32,
}

/// One world frame slot owns every mutable liquid descriptor and depth texel.
pub(in crate::device) struct LiquidFrameResources {
    buffer: vk::Buffer,
    allocation: Option<vk_mem::Allocation>,
    capacity: usize,
    stride: u64,
    depth_offset: u64,
    pool: vk::DescriptorPool,
    uniform_set: vk::DescriptorSet,
    material_layout: vk::DescriptorSetLayout,
    material_pool: vk::DescriptorPool,
    material_sets: Vec<vk::DescriptorSet>,
    draw_materials: Vec<usize>,
    samplers: [vk::Sampler; 2],
    filtering: WorldModelTextureFiltering,
    depths: [DepthImage; 3],
}

impl LiquidFrameResources {
    pub(in crate::device) const fn empty() -> Self {
        Self {
            buffer: vk::Buffer::null(),
            allocation: None,
            capacity: 0,
            stride: 0,
            depth_offset: 0,
            pool: vk::DescriptorPool::null(),
            uniform_set: vk::DescriptorSet::null(),
            material_layout: vk::DescriptorSetLayout::null(),
            material_pool: vk::DescriptorPool::null(),
            material_sets: Vec::new(),
            draw_materials: Vec::new(),
            samplers: [vk::Sampler::null(); 2],
            filtering: WorldModelTextureFiltering::Trilinear,
            depths: [
                DepthImage::empty(),
                DepthImage::empty(),
                DepthImage::empty(),
            ],
        }
    }

    /// Grows only this retired slot, preserving existing resources if creation fails.
    pub(in crate::device) fn ensure(
        &mut self,
        context: LiquidFrameCreateContext<'_>,
    ) -> Result<(), VulkanError> {
        if context.count == 0
            || (self.capacity >= context.count && self.filtering == context.filtering)
        {
            return Ok(());
        }
        let capacity = geometric_capacity(self.capacity, context.count);
        let retain_depths = self.capacity != 0;
        let retain_samplers = retain_depths && self.filtering == context.filtering;
        let mut replacement = Self::empty();
        if let Err(error) = replacement.create(&context, capacity, !retain_depths, !retain_samplers)
        {
            replacement.destroy(context.device, context.allocator);
            return Err(error);
        }
        // Fixed resources and texture sets do not depend on uniform capacity.
        // Transfer only after replacement allocation succeeds; the retired slot's
        // next write refreshes every used descriptor and procedural depth texel.
        if retain_depths {
            std::mem::swap(&mut replacement.depths, &mut self.depths);
        }
        if retain_samplers {
            std::mem::swap(&mut replacement.samplers, &mut self.samplers);
        }
        std::mem::swap(&mut replacement.material_pool, &mut self.material_pool);
        std::mem::swap(&mut replacement.material_sets, &mut self.material_sets);
        std::mem::swap(&mut replacement.draw_materials, &mut self.draw_materials);
        self.destroy(context.device, context.allocator);
        *self = replacement;
        Ok(())
    }

    /// Allocates checked dynamic ranges, immutable sampler state, and exact-size descriptor storage.
    fn create(
        &mut self,
        context: &LiquidFrameCreateContext<'_>,
        capacity: usize,
        create_depths: bool,
        create_samplers: bool,
    ) -> Result<(), VulkanError> {
        let device = context.device;
        let allocator = context.allocator;
        let layouts = context.layouts;
        self.material_layout = layouts[1];
        let alignment = context.alignment.max(1);
        self.filtering = context.filtering;
        self.stride = (LiquidShaderUniform::BYTE_SIZE as u64)
            .checked_add(alignment - 1)
            .map(|size| size / alignment * alignment)
            .ok_or(VulkanError::WorldFrameCapacity)?;
        self.depth_offset = (capacity as u64)
            .checked_mul(self.stride)
            .ok_or(VulkanError::WorldFrameCapacity)?;
        u32::try_from(self.depth_offset).map_err(|_| VulkanError::WorldFrameCapacity)?;
        let size = self
            .depth_offset
            .checked_add((3 * LiquidDepthTexture::BYTE_SIZE) as u64)
            .ok_or(VulkanError::WorldFrameCapacity)?;
        usize::try_from(size).map_err(|_| VulkanError::WorldFrameCapacity)?;
        let info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(vk::BufferUsageFlags::UNIFORM_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let allocation = vk_mem::AllocationCreateInfo {
            flags: vk_mem::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
            usage: vk_mem::MemoryUsage::AutoPreferHost,
            required_flags: vk::MemoryPropertyFlags::HOST_VISIBLE,
            ..Default::default()
        };
        // SAFETY: VMA binds a host-visible allocation covering the checked total extent.
        let (buffer, memory) = unsafe { allocator.create_buffer(&info, &allocation) }
            .map_err(|source| VulkanError::operation("create liquid frame buffer", source))?;
        self.buffer = buffer;
        self.allocation = Some(memory);
        if create_depths {
            for depth in &mut self.depths {
                depth.create(device, allocator)?;
            }
        }
        for (index, address) in [
            vk::SamplerAddressMode::CLAMP_TO_EDGE,
            vk::SamplerAddressMode::REPEAT,
        ]
        .into_iter()
        .enumerate()
        .filter(|_| create_samplers)
        {
            // 8A2450 keeps procedural depth linear/clamped; 4B9760 replaces
            // ordinary surface filtering with the global textureFilteringMode.
            let anisotropy = if index == 0 {
                1.0
            } else {
                self.filtering
                    .requested_anisotropy()
                    .min(context.maximum_anisotropy.max(1.0))
            };
            let info = vk::SamplerCreateInfo::default()
                .mag_filter(vk::Filter::LINEAR)
                .min_filter(vk::Filter::LINEAR)
                .mipmap_mode(if index == 0 || self.filtering.uses_linear_mips() {
                    vk::SamplerMipmapMode::LINEAR
                } else {
                    vk::SamplerMipmapMode::NEAREST
                })
                .anisotropy_enable(anisotropy > 1.0)
                .max_anisotropy(anisotropy)
                .address_mode_u(address)
                .address_mode_v(address)
                .address_mode_w(address)
                .max_lod(if index == 0 { 0.0 } else { vk::LOD_CLAMP_NONE });
            // SAFETY: Anisotropy is capped to the enabled device feature and limit.
            self.samplers[index] = unsafe { device.create_sampler(&info, None) }
                .map_err(|source| VulkanError::operation("create liquid sampler", source))?;
        }
        let sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC)
            .descriptor_count(1)];
        let info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(1)
            .pool_sizes(&sizes);
        // SAFETY: This pool owns only the slot's dynamic uniform descriptor.
        self.pool = unsafe { device.create_descriptor_pool(&info, None) }
            .map_err(|source| VulkanError::operation("create liquid descriptor pool", source))?;
        let requested = [layouts[0]];
        let info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(self.pool)
            .set_layouts(&requested);
        // SAFETY: The compatible layouts and exact-sized pool remain alive throughout allocation.
        let mut allocated =
            unsafe { device.allocate_descriptor_sets(&info) }.map_err(|source| {
                VulkanError::operation("allocate liquid frame descriptors", source)
            })?;
        self.uniform_set = allocated.remove(0);
        let uniform = [vk::DescriptorBufferInfo::default()
            .buffer(self.buffer)
            .range(LiquidShaderUniform::BYTE_SIZE as u64)];
        let writes = [vk::WriteDescriptorSet::default()
            .dst_set(self.uniform_set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC)
            .buffer_info(&uniform)];
        // SAFETY: The dynamic descriptor's base range is within the retained mapped buffer.
        unsafe { device.update_descriptor_sets(&writes, &[]) };
        self.capacity = capacity;
        Ok(())
    }

    /// Texture storage follows distinct image pairs, independently of draw count.
    /// A failed allocation leaves the retired slot's previous bank intact.
    fn ensure_materials(&mut self, device: &Device, required: usize) -> Result<(), VulkanError> {
        if self.material_sets.len() >= required {
            return Ok(());
        }
        let capacity = geometric_capacity(self.material_sets.len(), required);
        let count = u32::try_from(capacity).map_err(|_| VulkanError::WorldFrameCapacity)?;
        let sampled = count
            .checked_mul(2)
            .ok_or(VulkanError::WorldFrameCapacity)?;
        let sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(sampled)];
        let info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(count)
            .pool_sizes(&sizes);
        // SAFETY: Every material layout contains exactly two sampled images.
        let pool = unsafe { device.create_descriptor_pool(&info, None) }
            .map_err(|source| VulkanError::operation("create liquid material pool", source))?;
        let layouts = [self.material_layout];
        let info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(pool)
            .set_layouts(&layouts);
        let mut sets = Vec::with_capacity(capacity);
        // Keep the allocation shape fixed as the bank grows.
        for _ in 0..capacity {
            // SAFETY: The unsubmitted pool covers the complete bank; this call
            // requests one compatible set and no concurrent caller uses the pool.
            match unsafe { device.allocate_descriptor_sets(&info) } {
                Ok(allocated) => sets.extend(allocated),
                Err(source) => {
                    // SAFETY: All sets allocated so far remain unsubmitted.
                    unsafe { device.destroy_descriptor_pool(pool, None) };
                    return Err(VulkanError::operation(
                        "allocate liquid material descriptors",
                        source,
                    ));
                }
            }
        }
        if self.material_pool != vk::DescriptorPool::null() {
            // SAFETY: The slot fence retired every user of the old material bank.
            unsafe { device.destroy_descriptor_pool(self.material_pool, None) };
        }
        self.material_pool = pool;
        self.material_sets = sets;
        Ok(())
    }

    /// Writes a coherent snapshot after this slot's previous GPU use has completed.
    pub(in crate::device) fn write(
        &mut self,
        device: &Device,
        allocator: &vk_mem::Allocator,
        textures: &BlpTextureRegistry,
        frame: LiquidFrame<'_>,
    ) -> Result<(), VulkanError> {
        if frame.draws().is_empty() {
            return Ok(());
        }
        if frame.draws().len() > self.capacity {
            return Err(VulkanError::WorldFrameCapacity);
        }
        // Resolve every live view before mapping so errors cannot strand mapped memory.
        let mut materials = std::collections::HashMap::new();
        let mut images = Vec::new();
        self.draw_materials.clear();
        for draw in frame.draws() {
            let surface = textures
                .view(draw.surface())
                .ok_or(VulkanError::UnknownBlpTextureHandle)?;
            // Magma never reads binding zero. A valid surface descriptor there
            // keeps the shared layout complete without inventing a depth input.
            let depth = draw
                .material()
                .depth()
                .map_or(surface, |kind| self.depths[depth_index(kind)].view());
            let index = *materials.entry((depth, surface)).or_insert_with(|| {
                let index = images.len();
                images.push([
                    vk::DescriptorImageInfo::default()
                        .sampler(self.samplers[0])
                        .image_view(depth)
                        .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL),
                    vk::DescriptorImageInfo::default()
                        .sampler(self.samplers[1])
                        .image_view(surface)
                        .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL),
                ]);
                index
            });
            self.draw_materials.push(index);
        }
        self.ensure_materials(device, images.len())?;
        let allocation = self
            .allocation
            .as_mut()
            .ok_or(VulkanError::WorldFrameCapacity)?;
        // SAFETY: Creation requires host visibility and this slot has retired all readers.
        let destination = unsafe { allocator.map_memory(allocation) }
            .map_err(|source| VulkanError::operation("map liquid frame buffer", source))?;
        for (index, draw) in frame.draws().iter().enumerate() {
            let bytes = draw.uniform().to_bytes();
            // SAFETY: Capacity and stride checks cover this nonoverlapping uniform range.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    bytes.as_ptr(),
                    destination.add(index * self.stride as usize),
                    bytes.len(),
                )
            };
        }
        for (index, depth) in frame.depths().into_iter().enumerate() {
            let offset = self.depth_offset as usize + index * LiquidDepthTexture::BYTE_SIZE;
            // SAFETY: The allocation reserves all three complete procedural images after uniforms.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    depth.pixels_rgba().as_ptr(),
                    destination.add(offset),
                    LiquidDepthTexture::BYTE_SIZE,
                )
            };
        }
        let flushed = allocator
            .flush_allocation(allocation, 0, vk::WHOLE_SIZE)
            .map_err(|source| VulkanError::operation("flush liquid frame buffer", source));
        // SAFETY: Exactly this allocation was successfully mapped above, including flush failure paths.
        unsafe { allocator.unmap_memory(allocation) };
        flushed?;
        let writes = images
            .iter()
            .enumerate()
            .flat_map(|(index, images)| {
                let material = self.material_sets[index];
                images.iter().enumerate().map(move |(binding, image)| {
                    vk::WriteDescriptorSet::default()
                        .dst_set(material)
                        .dst_binding(binding as u32)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .image_info(std::slice::from_ref(image))
                })
            })
            .collect::<Vec<_>>();
        // SAFETY: Slot retirement excludes descriptor users; all borrowed image infos remain live.
        unsafe { device.update_descriptor_sets(&writes, &[]) };
        Ok(())
    }

    /// Copies current depth images before dynamic rendering starts in this slot.
    pub(in crate::device) fn record_uploads(&self, device: &Device, command: vk::CommandBuffer) {
        for (index, depth) in self.depths.iter().enumerate() {
            depth.upload(
                device,
                command,
                self.buffer,
                self.depth_offset + (index * LiquidDepthTexture::BYTE_SIZE) as u64,
            );
        }
    }

    /// Returns the compatible descriptor pair and checked dynamic uniform offset.
    pub(in crate::device) fn draw_sets(
        &self,
        index: usize,
    ) -> Result<([vk::DescriptorSet; 2], u32), VulkanError> {
        let material = self
            .draw_materials
            .get(index)
            .and_then(|material| self.material_sets.get(*material))
            .copied()
            .ok_or(VulkanError::WorldFrameCapacity)?;
        let offset = (index as u64)
            .checked_mul(self.stride)
            .and_then(|value| u32::try_from(value).ok())
            .ok_or(VulkanError::WorldFrameCapacity)?;
        Ok(([self.uniform_set, material], offset))
    }

    /// Releases all slot-owned children after its fence or renderer-wide idle.
    pub(in crate::device) fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        // SAFETY: The enclosing slot has retired every descriptor, sampler, and buffer use.
        unsafe {
            if self.pool != vk::DescriptorPool::null() {
                device.destroy_descriptor_pool(self.pool, None);
                self.pool = vk::DescriptorPool::null();
            }
            if self.material_pool != vk::DescriptorPool::null() {
                device.destroy_descriptor_pool(self.material_pool, None);
                self.material_pool = vk::DescriptorPool::null();
            }
            for sampler in &mut self.samplers {
                if *sampler != vk::Sampler::null() {
                    device.destroy_sampler(*sampler, None);
                    *sampler = vk::Sampler::null();
                }
            }
            if let Some(mut allocation) = self.allocation.take() {
                allocator.destroy_buffer(self.buffer, &mut allocation);
                self.buffer = vk::Buffer::null();
            }
        }
        for depth in &mut self.depths {
            depth.destroy(device, allocator);
        }
        self.material_sets.clear();
        self.draw_materials.clear();
        self.uniform_set = vk::DescriptorSet::null();
        self.capacity = 0;
    }
}
