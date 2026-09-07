//! Reusable unified-frame descriptors, mapped payload, depth, and synchronization.

#![allow(unsafe_code)]

use ash::{Device, vk};
use glam::Mat4;
use vk_mem::Alloc;

use crate::device::VulkanError;
use crate::device::capacity::geometric_capacity;
use crate::device::vulkan_liquid::LiquidFrameResources;
use crate::device::vulkan_m2_draw::{M2PreparedDraw, M2SceneLightBank};
use crate::device::vulkan_m2_pipeline::M2_MATERIAL_DESCRIPTOR_TYPE;
use crate::device::vulkan_world_model_draw::WorldModelPreparedDraw;
use crate::{
    M2MaterialUniform, M2ParticleRenderVertex, M2RibbonRenderVertex, M2SceneUniform,
    TerrainSceneUniform, WorldFrameScene, WorldModelMaterialUniform, WorldModelSceneUniform,
};

const DESCRIPTOR_SET_COUNT: usize = 8;
const BONE_TRANSFORM_BYTES: vk::DeviceSize = 64;

pub(super) struct FrameCreateContext<'a> {
    pub(super) device: &'a Device,
    pub(super) allocator: &'a vk_mem::Allocator,
    pub(super) descriptor_layouts: [vk::DescriptorSetLayout; DESCRIPTOR_SET_COUNT],
    pub(super) graphics_queue_family: u32,
    pub(super) slot_count: usize,
    pub(super) world_model_draw_capacity: usize,
    pub(super) m2_draw_capacity: usize,
    pub(super) bone_capacity: usize,
    pub(super) particle_vertex_capacity: usize,
    pub(super) particle_index_capacity: usize,
    pub(super) ribbon_vertex_capacity: usize,
    pub(super) uniform_alignment: vk::DeviceSize,
    pub(super) storage_alignment: vk::DeviceSize,
    pub(super) extent: (u32, u32),
    pub(super) depth_format: vk::Format,
}

#[derive(Clone, Copy)]
struct FrameBufferLayout {
    terrain_scene_offset: vk::DeviceSize,
    world_model_scene_offset: vk::DeviceSize,
    world_model_material_offset: vk::DeviceSize,
    world_model_material_stride: vk::DeviceSize,
    m2_scene_offset: vk::DeviceSize,
    m2_scene_stride: vk::DeviceSize,
    bone_offset: vk::DeviceSize,
    bone_bytes: vk::DeviceSize,
    m2_material_offset: vk::DeviceSize,
    m2_material_stride: vk::DeviceSize,
    particle_vertex_offset: vk::DeviceSize,
    particle_index_offset: vk::DeviceSize,
    ribbon_vertex_offset: vk::DeviceSize,
    total_bytes: vk::DeviceSize,
}

impl FrameBufferLayout {
    fn new(context: &FrameCreateContext<'_>) -> Result<Self, VulkanError> {
        let uniform_alignment = context.uniform_alignment.max(1);
        let terrain_scene_offset = 0;
        let world_model_scene_offset =
            align_up(TerrainSceneUniform::BYTE_SIZE as u64, uniform_alignment)?;
        let world_model_material_offset = align_up(
            world_model_scene_offset + WorldModelSceneUniform::BYTE_SIZE as u64,
            uniform_alignment,
        )?;
        let world_model_material_stride = align_up(
            WorldModelMaterialUniform::BYTE_SIZE as u64,
            uniform_alignment,
        )?;
        validate_dynamic_range(
            context.world_model_draw_capacity,
            world_model_material_stride,
        )?;
        let world_model_material_bytes = (context.world_model_draw_capacity.max(1) as u64)
            .checked_mul(world_model_material_stride)
            .ok_or(VulkanError::WorldFrameCapacity)?;
        let m2_scene_offset = align_up(
            world_model_material_offset
                .checked_add(world_model_material_bytes)
                .ok_or(VulkanError::WorldFrameCapacity)?,
            uniform_alignment,
        )?;
        let m2_scene_stride = align_up(M2SceneUniform::BYTE_SIZE as u64, uniform_alignment)?;
        let m2_scene_bytes = (M2SceneLightBank::COUNT as u64)
            .checked_mul(m2_scene_stride)
            .ok_or(VulkanError::WorldFrameCapacity)?;
        let bone_offset = align_up(
            m2_scene_offset
                .checked_add(m2_scene_bytes)
                .ok_or(VulkanError::WorldFrameCapacity)?,
            context.storage_alignment.max(16),
        )?;
        let bone_bytes = (context.bone_capacity.max(1) as u64)
            .checked_mul(BONE_TRANSFORM_BYTES)
            .ok_or(VulkanError::WorldFrameCapacity)?;
        let m2_material_offset = align_up(
            bone_offset
                .checked_add(bone_bytes)
                .ok_or(VulkanError::WorldFrameCapacity)?,
            uniform_alignment,
        )?;
        let m2_material_stride = align_up(M2MaterialUniform::BYTE_SIZE as u64, uniform_alignment)?;
        validate_dynamic_range(context.m2_draw_capacity, m2_material_stride)?;
        let m2_material_bytes = (context.m2_draw_capacity.max(1) as u64)
            .checked_mul(m2_material_stride)
            .ok_or(VulkanError::WorldFrameCapacity)?;
        let m2_end = m2_material_offset
            .checked_add(m2_material_bytes)
            .ok_or(VulkanError::WorldFrameCapacity)?;
        let particle_vertex_offset = align_up(m2_end, 4)?;
        let particle_vertex_bytes = (context.particle_vertex_capacity.max(1) as u64)
            .checked_mul(M2ParticleRenderVertex::BYTE_SIZE as u64)
            .ok_or(VulkanError::WorldFrameCapacity)?;
        let particle_index_offset = align_up(
            particle_vertex_offset
                .checked_add(particle_vertex_bytes)
                .ok_or(VulkanError::WorldFrameCapacity)?,
            4,
        )?;
        let particle_index_bytes = (context.particle_index_capacity.max(1) as u64)
            .checked_mul(size_of::<u32>() as u64)
            .ok_or(VulkanError::WorldFrameCapacity)?;
        let ribbon_vertex_offset = align_up(
            particle_index_offset
                .checked_add(particle_index_bytes)
                .ok_or(VulkanError::WorldFrameCapacity)?,
            4,
        )?;
        let ribbon_vertex_bytes = (context.ribbon_vertex_capacity.max(1) as u64)
            .checked_mul(M2RibbonRenderVertex::BYTE_SIZE as u64)
            .ok_or(VulkanError::WorldFrameCapacity)?;
        let total_bytes = ribbon_vertex_offset
            .checked_add(ribbon_vertex_bytes)
            .ok_or(VulkanError::WorldFrameCapacity)?;
        Ok(Self {
            terrain_scene_offset,
            world_model_scene_offset,
            world_model_material_offset,
            world_model_material_stride,
            m2_scene_offset,
            m2_scene_stride,
            bone_offset,
            bone_bytes,
            m2_material_offset,
            m2_material_stride,
            particle_vertex_offset,
            particle_index_offset,
            ribbon_vertex_offset,
            total_bytes,
        })
    }
}

pub(super) struct WorldFrameSlot {
    pub(super) liquids: LiquidFrameResources,
    buffer: vk::Buffer,
    buffer_allocation: Option<vk_mem::Allocation>,
    layout: FrameBufferLayout,
    descriptor_pool: vk::DescriptorPool,
    descriptor_sets: [vk::DescriptorSet; DESCRIPTOR_SET_COUNT],
    depth_image: vk::Image,
    depth_allocation: Option<vk_mem::Allocation>,
    depth_view: vk::ImageView,
    command_pool: vk::CommandPool,
    command_buffer: vk::CommandBuffer,
    image_available: vk::Semaphore,
    fence: vk::Fence,
}

impl WorldFrameSlot {
    fn create(
        context: &FrameCreateContext<'_>,
        layout: FrameBufferLayout,
    ) -> Result<Self, VulkanError> {
        let mut slot = Self::empty(layout);
        let result = (|| {
            slot.create_buffer(context)?;
            slot.create_descriptors(context)?;
            slot.create_depth(context)?;
            slot.create_commands(context)?;
            slot.create_synchronization(context.device)?;
            Ok(())
        })();
        if let Err(error) = result {
            slot.destroy(context.device, context.allocator);
            return Err(error);
        }
        Ok(slot)
    }

    pub(super) const fn image_available(&self) -> vk::Semaphore {
        self.image_available
    }

    pub(super) const fn fence(&self) -> vk::Fence {
        self.fence
    }

    pub(super) const fn command_buffer(&self) -> vk::CommandBuffer {
        self.command_buffer
    }

    pub(super) const fn descriptor_sets(&self) -> [vk::DescriptorSet; DESCRIPTOR_SET_COUNT] {
        self.descriptor_sets
    }

    pub(super) const fn depth_image(&self) -> vk::Image {
        self.depth_image
    }

    pub(super) const fn depth_view(&self) -> vk::ImageView {
        self.depth_view
    }

    pub(super) const fn world_model_material_stride(&self) -> vk::DeviceSize {
        self.layout.world_model_material_stride
    }

    pub(super) const fn m2_material_stride(&self) -> vk::DeviceSize {
        self.layout.m2_material_stride
    }

    pub(super) const fn ribbon_vertex_buffer(&self) -> (vk::Buffer, vk::DeviceSize) {
        (self.buffer, self.layout.ribbon_vertex_offset)
    }

    pub(super) const fn particle_vertex_buffer(&self) -> (vk::Buffer, vk::DeviceSize) {
        (self.buffer, self.layout.particle_vertex_offset)
    }

    pub(super) const fn particle_index_buffer(&self) -> (vk::Buffer, vk::DeviceSize) {
        (self.buffer, self.layout.particle_index_offset)
    }

    pub(super) fn wait_and_reset(&self, device: &Device) -> Result<(), VulkanError> {
        // SAFETY: This slot owns both objects and prior use is fence-protected.
        unsafe {
            device
                .wait_for_fences(&[self.fence], true, u64::MAX)
                .map_err(|source| VulkanError::operation("wait for world frame slot", source))?;
            device
                .reset_command_pool(self.command_pool, vk::CommandPoolResetFlags::empty())
                .map_err(|source| {
                    VulkanError::operation("reset world frame command pool", source)
                })?;
        }
        Ok(())
    }

    pub(super) fn reset_fence(&self, device: &Device) -> Result<(), VulkanError> {
        // SAFETY: Prior work was waited and no submission owns the fence now.
        unsafe { device.reset_fences(&[self.fence]) }
            .map_err(|source| VulkanError::operation("reset world frame fence", source))
    }

    pub(super) fn restore_signaled_fence(&mut self, device: &Device) -> Result<(), VulkanError> {
        // SAFETY: Queue submission failed, so the old fence is unowned.
        unsafe { device.destroy_fence(self.fence, None) };
        self.fence = vk::Fence::null();
        let info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        // SAFETY: Default internal fence has no external payload.
        self.fence = unsafe { device.create_fence(&info, None) }
            .map_err(|source| VulkanError::operation("restore world frame fence", source))?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn write(
        &mut self,
        allocator: &vk_mem::Allocator,
        scene: WorldFrameScene<'_>,
        bone_transforms: &[Mat4],
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
                copy_bytes(
                    destination,
                    indexed_offset(
                        self.layout.m2_scene_offset,
                        self.layout.m2_scene_stride,
                        light_bank.index(),
                    )?,
                    &scene.m2(light_bank).to_bytes(),
                    self.layout.total_bytes,
                )?;
            }
            if bone_transforms.is_empty() {
                copy_bytes(
                    destination,
                    self.layout.bone_offset,
                    &[0_u8; 64],
                    self.layout.total_bytes,
                )?;
            } else {
                for (index, transform) in bone_transforms.iter().copied().enumerate() {
                    copy_bytes(
                        destination,
                        indexed_offset(self.layout.bone_offset, BONE_TRANSFORM_BYTES, index)?,
                        &matrix_bytes(transform),
                        self.layout.total_bytes,
                    )?;
                }
            }
            for (index, draw) in world_model_draws.iter().copied().enumerate() {
                copy_bytes(
                    destination,
                    indexed_offset(
                        self.layout.world_model_material_offset,
                        self.layout.world_model_material_stride,
                        index,
                    )?,
                    &draw.material().to_bytes(),
                    self.layout.total_bytes,
                )?;
            }
            for (index, draw) in m2_draws.iter().copied().enumerate() {
                copy_bytes(
                    destination,
                    indexed_offset(
                        self.layout.m2_material_offset,
                        self.layout.m2_material_stride,
                        index,
                    )?,
                    &draw.material().to_bytes(),
                    self.layout.total_bytes,
                )?;
            }
            for (index, vertex) in particle_vertices.iter().copied().enumerate() {
                copy_bytes(
                    destination,
                    indexed_offset(
                        self.layout.particle_vertex_offset,
                        M2ParticleRenderVertex::BYTE_SIZE as u64,
                        index,
                    )?,
                    &vertex.to_bytes(),
                    self.layout.total_bytes,
                )?;
            }
            for (index, particle_index) in particle_indices.iter().copied().enumerate() {
                copy_bytes(
                    destination,
                    indexed_offset(
                        self.layout.particle_index_offset,
                        size_of::<u32>() as u64,
                        index,
                    )?,
                    &particle_index.to_le_bytes(),
                    self.layout.total_bytes,
                )?;
            }
            for (index, vertex) in ribbon_vertices.iter().copied().enumerate() {
                copy_bytes(
                    destination,
                    indexed_offset(
                        self.layout.ribbon_vertex_offset,
                        M2RibbonRenderVertex::BYTE_SIZE as u64,
                        index,
                    )?,
                    &vertex.to_bytes(),
                    self.layout.total_bytes,
                )?;
            }
            allocator
                .flush_allocation(allocation, 0, self.layout.total_bytes)
                .map_err(|source| VulkanError::operation("flush world frame buffer", source))
        })()
    }

    fn create_buffer(&mut self, context: &FrameCreateContext<'_>) -> Result<(), VulkanError> {
        let info = vk::BufferCreateInfo::default()
            .size(self.layout.total_bytes)
            .usage(
                vk::BufferUsageFlags::UNIFORM_BUFFER
                    | vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::VERTEX_BUFFER
                    | vk::BufferUsageFlags::INDEX_BUFFER,
            )
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let allocation_info = vk_mem::AllocationCreateInfo {
            flags: vk_mem::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE
                | vk_mem::AllocationCreateFlags::MAPPED,
            usage: vk_mem::MemoryUsage::AutoPreferHost,
            required_flags: vk::MemoryPropertyFlags::HOST_VISIBLE,
            ..Default::default()
        };
        // SAFETY: VMA binds the returned allocation to the new buffer.
        let (buffer, allocation) =
            unsafe { context.allocator.create_buffer(&info, &allocation_info) }
                .map_err(|source| VulkanError::operation("create world frame buffer", source))?;
        self.buffer = buffer;
        self.buffer_allocation = Some(allocation);
        Ok(())
    }

    fn create_descriptors(&mut self, context: &FrameCreateContext<'_>) -> Result<(), VulkanError> {
        let pool_sizes = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(5),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC)
                .descriptor_count(2),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1),
        ];
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(DESCRIPTOR_SET_COUNT as u32)
            .pool_sizes(&pool_sizes);
        // SAFETY: Counts exactly cover the six fixed layouts.
        self.descriptor_pool = unsafe { context.device.create_descriptor_pool(&pool_info, None) }
            .map_err(|source| {
            VulkanError::operation("create world frame descriptor pool", source)
        })?;
        let allocate_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(self.descriptor_pool)
            .set_layouts(&context.descriptor_layouts);
        // SAFETY: Pool and layouts are live and compatible.
        let sets = unsafe { context.device.allocate_descriptor_sets(&allocate_info) }
            .map_err(|source| VulkanError::operation("allocate world frame descriptors", source))?;
        if sets.len() != DESCRIPTOR_SET_COUNT {
            return Err(VulkanError::WorldFrameCapacity);
        }
        self.descriptor_sets.copy_from_slice(&sets);
        let infos = [
            buffer_info(
                self.buffer,
                self.layout.terrain_scene_offset,
                TerrainSceneUniform::BYTE_SIZE,
            ),
            buffer_info(
                self.buffer,
                self.layout.world_model_scene_offset,
                WorldModelSceneUniform::BYTE_SIZE,
            ),
            buffer_info(
                self.buffer,
                self.layout.world_model_material_offset,
                WorldModelMaterialUniform::BYTE_SIZE,
            ),
            buffer_info(
                self.buffer,
                self.layout.m2_scene_offset,
                M2SceneUniform::BYTE_SIZE,
            ),
            buffer_info(
                self.buffer,
                self.layout.m2_scene_offset + self.layout.m2_scene_stride,
                M2SceneUniform::BYTE_SIZE,
            ),
            buffer_info(
                self.buffer,
                self.layout.m2_scene_offset + self.layout.m2_scene_stride * 2,
                M2SceneUniform::BYTE_SIZE,
            ),
            vk::DescriptorBufferInfo::default()
                .buffer(self.buffer)
                .offset(self.layout.bone_offset)
                .range(self.layout.bone_bytes),
            buffer_info(
                self.buffer,
                self.layout.m2_material_offset,
                M2MaterialUniform::BYTE_SIZE,
            ),
        ];
        let descriptor_types = [
            vk::DescriptorType::UNIFORM_BUFFER,
            vk::DescriptorType::UNIFORM_BUFFER,
            vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC,
            vk::DescriptorType::UNIFORM_BUFFER,
            vk::DescriptorType::UNIFORM_BUFFER,
            vk::DescriptorType::UNIFORM_BUFFER,
            vk::DescriptorType::STORAGE_BUFFER,
            M2_MATERIAL_DESCRIPTOR_TYPE,
        ];
        for index in 0..DESCRIPTOR_SET_COUNT {
            let buffer_infos = [infos[index]];
            let write = vk::WriteDescriptorSet::default()
                .dst_set(self.descriptor_sets[index])
                .dst_binding(0)
                .descriptor_type(descriptor_types[index])
                .buffer_info(&buffer_infos);
            // SAFETY: Each range lies within the live combined buffer.
            unsafe { context.device.update_descriptor_sets(&[write], &[]) };
        }
        Ok(())
    }

    fn create_depth(&mut self, context: &FrameCreateContext<'_>) -> Result<(), VulkanError> {
        let info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(context.depth_format)
            .extent(vk::Extent3D {
                width: context.extent.0,
                height: context.extent.1,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let allocation_info = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::AutoPreferDevice,
            required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
            ..Default::default()
        };
        // SAFETY: VMA binds the allocation to the exact image.
        let (image, allocation) =
            unsafe { context.allocator.create_image(&info, &allocation_info) }.map_err(
                |source| VulkanError::operation("create world frame depth image", source),
            )?;
        self.depth_image = image;
        self.depth_allocation = Some(allocation);
        let range = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL)
            .level_count(1)
            .layer_count(1);
        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(context.depth_format)
            .subresource_range(range);
        // SAFETY: The view covers the image's sole mip and layer.
        self.depth_view = unsafe { context.device.create_image_view(&view_info, None) }
            .map_err(|source| VulkanError::operation("create world frame depth view", source))?;
        Ok(())
    }

    fn create_commands(&mut self, context: &FrameCreateContext<'_>) -> Result<(), VulkanError> {
        let pool_info = vk::CommandPoolCreateInfo::default()
            .flags(vk::CommandPoolCreateFlags::TRANSIENT)
            .queue_family_index(context.graphics_queue_family);
        // SAFETY: The queue family was enabled on this device.
        self.command_pool = unsafe { context.device.create_command_pool(&pool_info, None) }
            .map_err(|source| VulkanError::operation("create world frame command pool", source))?;
        let info = vk::CommandBufferAllocateInfo::default()
            .command_pool(self.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        // SAFETY: The pool is live and uniquely owned by this slot.
        self.command_buffer = unsafe { context.device.allocate_command_buffers(&info) }
            .map_err(|source| {
                VulkanError::operation("allocate world frame command buffer", source)
            })?
            .first()
            .copied()
            .ok_or(VulkanError::WorldFrameCapacity)?;
        Ok(())
    }

    fn create_synchronization(&mut self, device: &Device) -> Result<(), VulkanError> {
        // SAFETY: Default binary semaphore has no borrowed state.
        self.image_available =
            unsafe { device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None) }.map_err(
                |source| VulkanError::operation("create world frame acquire semaphore", source),
            )?;
        let info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        // SAFETY: Default internal fence has no borrowed state.
        self.fence = unsafe { device.create_fence(&info, None) }
            .map_err(|source| VulkanError::operation("create world frame fence", source))?;
        Ok(())
    }

    fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        self.liquids.destroy(device, allocator);
        // SAFETY: The caller idles the device before destruction/rebuild.
        unsafe {
            if self.fence != vk::Fence::null() {
                device.destroy_fence(self.fence, None);
                self.fence = vk::Fence::null();
            }
            if self.image_available != vk::Semaphore::null() {
                device.destroy_semaphore(self.image_available, None);
                self.image_available = vk::Semaphore::null();
            }
            if self.command_pool != vk::CommandPool::null() {
                device.destroy_command_pool(self.command_pool, None);
                self.command_pool = vk::CommandPool::null();
                self.command_buffer = vk::CommandBuffer::null();
            }
            if self.depth_view != vk::ImageView::null() {
                device.destroy_image_view(self.depth_view, None);
                self.depth_view = vk::ImageView::null();
            }
            if let Some(mut allocation) = self.depth_allocation.take() {
                allocator.destroy_image(self.depth_image, &mut allocation);
                self.depth_image = vk::Image::null();
            }
            if self.descriptor_pool != vk::DescriptorPool::null() {
                device.destroy_descriptor_pool(self.descriptor_pool, None);
                self.descriptor_pool = vk::DescriptorPool::null();
                self.descriptor_sets = [vk::DescriptorSet::null(); DESCRIPTOR_SET_COUNT];
            }
            if let Some(mut allocation) = self.buffer_allocation.take() {
                allocator.destroy_buffer(self.buffer, &mut allocation);
                self.buffer = vk::Buffer::null();
            }
        }
    }

    const fn empty(layout: FrameBufferLayout) -> Self {
        Self {
            liquids: LiquidFrameResources::empty(),
            buffer: vk::Buffer::null(),
            buffer_allocation: None,
            layout,
            descriptor_pool: vk::DescriptorPool::null(),
            descriptor_sets: [vk::DescriptorSet::null(); DESCRIPTOR_SET_COUNT],
            depth_image: vk::Image::null(),
            depth_allocation: None,
            depth_view: vk::ImageView::null(),
            command_pool: vk::CommandPool::null(),
            command_buffer: vk::CommandBuffer::null(),
            image_available: vk::Semaphore::null(),
            fence: vk::Fence::null(),
        }
    }
}

#[derive(Default)]
pub(super) struct WorldFrameResources {
    slots: Vec<WorldFrameSlot>,
    present_semaphores: Vec<vk::Semaphore>,
    next_slot: usize,
    world_model_draw_capacity: usize,
    m2_draw_capacity: usize,
    bone_capacity: usize,
    particle_vertex_capacity: usize,
    particle_index_capacity: usize,
    ribbon_vertex_capacity: usize,
    extent: (u32, u32),
}

impl WorldFrameResources {
    pub(super) fn ensure(&mut self, context: FrameCreateContext<'_>) -> Result<(), VulkanError> {
        if !self.slots.is_empty()
            && self.slots.len() == context.slot_count
            && self.world_model_draw_capacity >= context.world_model_draw_capacity
            && self.m2_draw_capacity >= context.m2_draw_capacity
            && self.bone_capacity >= context.bone_capacity
            && self.particle_vertex_capacity >= context.particle_vertex_capacity
            && self.particle_index_capacity >= context.particle_index_capacity
            && self.ribbon_vertex_capacity >= context.ribbon_vertex_capacity
            && self.extent == context.extent
        {
            return Ok(());
        }
        if context.slot_count == 0 {
            return Err(VulkanError::WorldFrameCapacity);
        }
        // SAFETY: Growth invalidates descriptors and depth images; idle retires all use.
        unsafe { context.device.device_wait_idle() }
            .map_err(|source| VulkanError::operation("idle before world frame growth", source))?;
        let world_model_draw_capacity = geometric_capacity(
            self.world_model_draw_capacity,
            context.world_model_draw_capacity,
        );
        let m2_draw_capacity = geometric_capacity(self.m2_draw_capacity, context.m2_draw_capacity);
        let bone_capacity = geometric_capacity(self.bone_capacity, context.bone_capacity);
        let particle_vertex_capacity = geometric_capacity(
            self.particle_vertex_capacity,
            context.particle_vertex_capacity,
        );
        let particle_index_capacity = geometric_capacity(
            self.particle_index_capacity,
            context.particle_index_capacity,
        );
        let ribbon_vertex_capacity =
            geometric_capacity(self.ribbon_vertex_capacity, context.ribbon_vertex_capacity);
        tracing::info!(
            required_world_model_draws = context.world_model_draw_capacity,
            required_m2_draws = context.m2_draw_capacity,
            required_bones = context.bone_capacity,
            required_particle_vertices = context.particle_vertex_capacity,
            required_particle_indices = context.particle_index_capacity,
            required_ribbon_vertices = context.ribbon_vertex_capacity,
            world_model_draw_capacity,
            m2_draw_capacity,
            bone_capacity,
            particle_vertex_capacity,
            particle_index_capacity,
            ribbon_vertex_capacity,
            "growing unified Vulkan frame resources"
        );
        self.destroy(context.device, context.allocator);
        let expanded = FrameCreateContext {
            world_model_draw_capacity,
            m2_draw_capacity,
            bone_capacity,
            particle_vertex_capacity,
            particle_index_capacity,
            ribbon_vertex_capacity,
            ..context
        };
        let layout = FrameBufferLayout::new(&expanded)?;
        let mut slots = Vec::with_capacity(expanded.slot_count);
        for _ in 0..expanded.slot_count {
            match WorldFrameSlot::create(&expanded, layout) {
                Ok(slot) => slots.push(slot),
                Err(error) => {
                    for slot in &mut slots {
                        slot.destroy(expanded.device, expanded.allocator);
                    }
                    return Err(error);
                }
            }
        }
        let mut present_semaphores = Vec::with_capacity(expanded.slot_count);
        for _ in 0..expanded.slot_count {
            // SAFETY: Default binary semaphore has no borrowed state.
            match unsafe {
                expanded
                    .device
                    .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
            } {
                Ok(semaphore) => present_semaphores.push(semaphore),
                Err(source) => {
                    // SAFETY: No created semaphore has been submitted.
                    unsafe {
                        for semaphore in present_semaphores.drain(..).rev() {
                            expanded.device.destroy_semaphore(semaphore, None);
                        }
                    }
                    for slot in &mut slots {
                        slot.destroy(expanded.device, expanded.allocator);
                    }
                    return Err(VulkanError::operation(
                        "create world presentation semaphore",
                        source,
                    ));
                }
            }
        }
        self.slots = slots;
        self.present_semaphores = present_semaphores;
        self.world_model_draw_capacity = world_model_draw_capacity;
        self.m2_draw_capacity = m2_draw_capacity;
        self.bone_capacity = bone_capacity;
        self.particle_vertex_capacity = particle_vertex_capacity;
        self.particle_index_capacity = particle_index_capacity;
        self.ribbon_vertex_capacity = ribbon_vertex_capacity;
        self.extent = expanded.extent;
        self.next_slot = 0;
        Ok(())
    }

    pub(super) fn next_slot_index(&mut self) -> Result<usize, VulkanError> {
        if self.slots.is_empty() {
            return Err(VulkanError::WorldFrameCapacity);
        }
        let index = self.next_slot;
        self.next_slot = (self.next_slot + 1) % self.slots.len();
        Ok(index)
    }

    pub(super) fn slot_mut(&mut self, index: usize) -> Result<&mut WorldFrameSlot, VulkanError> {
        self.slots
            .get_mut(index)
            .ok_or(VulkanError::WorldFrameCapacity)
    }

    pub(super) fn present_semaphore(&self, image_index: u32) -> Result<vk::Semaphore, VulkanError> {
        self.present_semaphores
            .get(image_index as usize)
            .copied()
            .ok_or(VulkanError::WorldFrameCapacity)
    }

    pub(super) fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        // SAFETY: Caller guarantees submitted use is retired.
        unsafe {
            for semaphore in self.present_semaphores.drain(..).rev() {
                device.destroy_semaphore(semaphore, None);
            }
        }
        for mut slot in self.slots.drain(..).rev() {
            slot.destroy(device, allocator);
        }
        self.next_slot = 0;
        self.world_model_draw_capacity = 0;
        self.m2_draw_capacity = 0;
        self.bone_capacity = 0;
        self.particle_vertex_capacity = 0;
        self.particle_index_capacity = 0;
        self.ribbon_vertex_capacity = 0;
        self.extent = (0, 0);
    }
}

fn buffer_info(
    buffer: vk::Buffer,
    offset: vk::DeviceSize,
    byte_count: usize,
) -> vk::DescriptorBufferInfo {
    vk::DescriptorBufferInfo::default()
        .buffer(buffer)
        .offset(offset)
        .range(byte_count as u64)
}

fn validate_dynamic_range(count: usize, stride: u64) -> Result<(), VulkanError> {
    let last = (count.saturating_sub(1) as u64)
        .checked_mul(stride)
        .ok_or(VulkanError::WorldFrameCapacity)?;
    u32::try_from(last).map_err(|_source| VulkanError::WorldFrameCapacity)?;
    Ok(())
}

fn align_up(value: u64, alignment: u64) -> Result<u64, VulkanError> {
    let alignment = alignment.max(1);
    value
        .checked_add(alignment - 1)
        .map(|sum| sum / alignment * alignment)
        .ok_or(VulkanError::WorldFrameCapacity)
}

fn indexed_offset(base: u64, stride: u64, index: usize) -> Result<u64, VulkanError> {
    (index as u64)
        .checked_mul(stride)
        .and_then(|offset| base.checked_add(offset))
        .ok_or(VulkanError::WorldFrameCapacity)
}

fn copy_bytes(
    destination: *mut u8,
    offset: u64,
    bytes: &[u8],
    total: u64,
) -> Result<(), VulkanError> {
    let count = bytes.len() as u64;
    if offset.checked_add(count).is_none_or(|end| end > total) {
        return Err(VulkanError::WorldFrameCapacity);
    }
    let offset = usize::try_from(offset).map_err(|_source| VulkanError::WorldFrameCapacity)?;
    // SAFETY: Validated range lies within mapped memory and cannot overlap source.
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), destination.add(offset), bytes.len()) };
    Ok(())
}

fn matrix_bytes(matrix: Mat4) -> [u8; 64] {
    let mut bytes = [0_u8; 64];
    for (index, value) in matrix.to_cols_array().into_iter().enumerate() {
        let offset = index * 4;
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}
