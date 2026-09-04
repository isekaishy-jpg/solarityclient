//! One-shot transfer from explicit CPU bytes into device-local M2 buffers.

#![allow(unsafe_code)]

use ash::{Device, vk};
use vk_mem::Alloc;

use crate::device::VulkanError;
use crate::model::M2MeshPlan;

use super::types::M2MeshResourceInfo;

/// Borrowed Vulkan graph required to create and retire one mesh transfer.
#[derive(Clone, Copy)]
pub(in crate::device) struct MeshUploadContext<'a> {
    pub(in crate::device) device: &'a Device,
    pub(in crate::device) allocator: &'a vk_mem::Allocator,
    pub(in crate::device) graphics_queue: vk::Queue,
    pub(in crate::device) graphics_queue_family: u32,
}

/// One VMA-owned buffer represented without borrowing its allocator parent.
struct AllocatedBuffer {
    handle: vk::Buffer,
    allocation: Option<vk_mem::Allocation>,
}

impl AllocatedBuffer {
    /// Allocates one bound buffer from the requested Vulkan memory class.
    fn create(
        allocator: &vk_mem::Allocator,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
        allocation_info: &vk_mem::AllocationCreateInfo,
        operation: &'static str,
    ) -> Result<Self, VulkanError> {
        let buffer_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        // SAFETY: Both create infos are valid for a live allocator and VMA binds
        // the returned allocation to the new buffer before returning it.
        let (handle, allocation) =
            unsafe { allocator.create_buffer(&buffer_info, allocation_info) }
                .map_err(|source| VulkanError::operation(operation, source))?;
        Ok(Self {
            handle,
            allocation: Some(allocation),
        })
    }

    /// Frees the allocation and its bound Vulkan buffer exactly once.
    fn destroy(&mut self, allocator: &vk_mem::Allocator) {
        if let Some(mut allocation) = self.allocation.take() {
            // SAFETY: This allocation was returned with `self.handle` by this
            // allocator and no command references it after transfer retirement.
            unsafe { allocator.destroy_buffer(self.handle, &mut allocation) };
            self.handle = vk::Buffer::null();
        }
    }
}

/// Permanent geometry buffers retained by the renderer registry.
pub(super) struct GpuM2Mesh {
    buffers: GpuMeshBuffers,
    info: M2MeshResourceInfo,
}

/// Shared device-local vertex/index allocation used by typed mesh registries.
pub(in crate::device) struct GpuMeshBuffers {
    vertex_buffer: AllocatedBuffer,
    index_buffer: AllocatedBuffer,
}

impl GpuM2Mesh {
    /// Returns the public, driver-independent description of this allocation.
    pub(super) const fn info(&self) -> &M2MeshResourceInfo {
        &self.info
    }

    /// Returns both live device-local buffers for command recording.
    pub(super) const fn buffers(&self) -> (vk::Buffer, vk::Buffer) {
        self.buffers.buffers()
    }

    /// Releases both children before the renderer drops VMA.
    pub(super) fn destroy(&mut self, allocator: &vk_mem::Allocator) {
        self.buffers.destroy(allocator);
    }
}

impl GpuMeshBuffers {
    /// Returns both live device-local buffers for command recording.
    pub(in crate::device) const fn buffers(&self) -> (vk::Buffer, vk::Buffer) {
        (self.vertex_buffer.handle, self.index_buffer.handle)
    }

    /// Releases both children before the renderer drops VMA.
    pub(in crate::device) fn destroy(&mut self, allocator: &vk_mem::Allocator) {
        self.index_buffer.destroy(allocator);
        self.vertex_buffer.destroy(allocator);
    }
}

/// Partial permanent-resource construction guard for every failure path.
struct MeshGuard<'a> {
    allocator: &'a vk_mem::Allocator,
    buffers: Option<GpuMeshBuffers>,
}

impl MeshGuard<'_> {
    /// Transfers permanent ownership after recording has completed.
    fn finish(mut self) -> Result<GpuMeshBuffers, VulkanError> {
        self.buffers.take().ok_or_else(|| {
            VulkanError::operation("finish mesh upload", "mesh buffers are unavailable")
        })
    }
}

impl Drop for MeshGuard<'_> {
    /// Cleans both device buffers if any later allocation or command fails.
    fn drop(&mut self) {
        if let Some(buffers) = self.buffers.as_mut() {
            buffers.destroy(self.allocator);
        }
    }
}

/// Temporary host upload and command objects owned until successful submission.
struct TransferResources<'a> {
    context: MeshUploadContext<'a>,
    staging: AllocatedBuffer,
    command_pool: vk::CommandPool,
    fence: vk::Fence,
}

/// Submitted staging and command resources retained until their fence completes.
pub(super) struct DeferredMeshTransfer {
    staging: AllocatedBuffer,
    command_pool: vk::CommandPool,
    fence: vk::Fence,
}

impl DeferredMeshTransfer {
    /// Polls retirement without waiting on the presentation thread.
    pub(super) fn is_complete(&self, device: &Device) -> Result<bool, VulkanError> {
        // SAFETY: This transfer uniquely owns the submitted fence until retirement.
        unsafe { device.get_fence_status(self.fence) }
            .map_err(|source| VulkanError::operation("poll M2 buffer transfer", source))
    }

    /// Waits for callers whose mesh registry still requires synchronous admission.
    fn wait(&self, device: &Device) -> Result<(), VulkanError> {
        // SAFETY: The fence and all referenced resources remain owned through this wait.
        unsafe { device.wait_for_fences(&[self.fence], true, u64::MAX) }
            .map_err(|source| VulkanError::operation("wait for mesh buffer transfer", source))
    }

    /// Releases staging after fence completion or renderer-wide device idle.
    pub(super) fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        // SAFETY: Callers have retired this submission before releasing its
        // command pool, fence, and source allocation. Device loss also ends use.
        unsafe {
            if self.fence != vk::Fence::null() {
                device.destroy_fence(self.fence, None);
                self.fence = vk::Fence::null();
            }
            if self.command_pool != vk::CommandPool::null() {
                device.destroy_command_pool(self.command_pool, None);
                self.command_pool = vk::CommandPool::null();
            }
        }
        self.staging.destroy(allocator);
    }
}

impl<'a> TransferResources<'a> {
    /// Creates and fills one sequential staging allocation.
    fn create(context: MeshUploadContext<'a>, bytes: &[u8]) -> Result<Self, VulkanError> {
        let size = u64::try_from(bytes.len())
            .map_err(|source| VulkanError::operation("convert M2 staging size", source))?;
        let allocation_info = vk_mem::AllocationCreateInfo {
            flags: vk_mem::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
            usage: vk_mem::MemoryUsage::AutoPreferHost,
            required_flags: vk::MemoryPropertyFlags::HOST_VISIBLE,
            ..Default::default()
        };
        let staging = AllocatedBuffer::create(
            context.allocator,
            size,
            vk::BufferUsageFlags::TRANSFER_SRC,
            &allocation_info,
            "create M2 staging buffer",
        )?;
        let mut resources = Self {
            context,
            staging,
            command_pool: vk::CommandPool::null(),
            fence: vk::Fence::null(),
        };
        resources.write(bytes)?;
        resources.create_submission_objects()?;
        Ok(resources)
    }

    /// Copies bytes into mapped host memory and flushes noncoherent ranges.
    fn write(&mut self, bytes: &[u8]) -> Result<(), VulkanError> {
        let allocation = self.staging.allocation.as_mut().ok_or_else(|| {
            VulkanError::operation("access M2 staging allocation", "allocation is unavailable")
        })?;
        // SAFETY: The allocation is host-visible and exactly covers `bytes`.
        let destination = unsafe { self.context.allocator.map_memory(allocation) }
            .map_err(|source| VulkanError::operation("map M2 staging buffer", source))?;
        // SAFETY: Both pointers are valid for `bytes.len()` nonoverlapping bytes.
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), destination, bytes.len()) };
        let flush_result = self
            .context
            .allocator
            .flush_allocation(allocation, 0, vk::WHOLE_SIZE)
            .map_err(|source| VulkanError::operation("flush M2 staging buffer", source));
        // SAFETY: This balances the successful map regardless of flush result.
        unsafe { self.context.allocator.unmap_memory(allocation) };
        flush_result
    }

    /// Creates a transient pool, one primary buffer, and its retirement fence.
    fn create_submission_objects(&mut self) -> Result<(), VulkanError> {
        let pool_info = vk::CommandPoolCreateInfo::default()
            .flags(vk::CommandPoolCreateFlags::TRANSIENT)
            .queue_family_index(self.context.graphics_queue_family);
        // SAFETY: The selected graphics family was enabled on this live device.
        self.command_pool = unsafe { self.context.device.create_command_pool(&pool_info, None) }
            .map_err(|source| VulkanError::operation("create M2 transfer command pool", source))?;
        // SAFETY: Default fence creation has no borrowed state.
        self.fence = unsafe {
            self.context
                .device
                .create_fence(&vk::FenceCreateInfo::default(), None)
        }
        .map_err(|source| VulkanError::operation("create M2 transfer fence", source))?;
        Ok(())
    }

    /// Allocates the sole primary command buffer from the live transient pool.
    fn command_buffer(&self) -> Result<vk::CommandBuffer, VulkanError> {
        let info = vk::CommandBufferAllocateInfo::default()
            .command_pool(self.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        // SAFETY: The pool is live and no allocation from it outlives this owner.
        let buffers =
            unsafe { self.context.device.allocate_command_buffers(&info) }.map_err(|source| {
                VulkanError::operation("allocate M2 transfer command buffer", source)
            })?;
        buffers.first().copied().ok_or_else(|| {
            VulkanError::operation(
                "allocate M2 transfer command buffer",
                "driver returned no command buffer",
            )
        })
    }

    /// Submits recorded copies and transfers staging ownership without a host wait.
    fn submit_and_defer(
        mut self,
        command_buffer: vk::CommandBuffer,
    ) -> Result<DeferredMeshTransfer, VulkanError> {
        let command_info = vk::CommandBufferSubmitInfo::default().command_buffer(command_buffer);
        let command_infos = [command_info];
        let submit_info = vk::SubmitInfo2::default().command_buffer_infos(&command_infos);
        // SAFETY: The command buffer is executable and all referenced buffers
        // remain live until this owner's fence completes.
        unsafe {
            self.context.device.queue_submit2(
                self.context.graphics_queue,
                &[submit_info],
                self.fence,
            )
        }
        .map_err(|source| VulkanError::operation("submit M2 buffer transfer", source))?;
        Ok(DeferredMeshTransfer {
            staging: AllocatedBuffer {
                handle: std::mem::replace(&mut self.staging.handle, vk::Buffer::null()),
                allocation: self.staging.allocation.take(),
            },
            command_pool: std::mem::replace(&mut self.command_pool, vk::CommandPool::null()),
            fence: std::mem::replace(&mut self.fence, vk::Fence::null()),
        })
    }
}

impl Drop for TransferResources<'_> {
    /// Releases command state before the mapped-buffer allocation parent.
    fn drop(&mut self) {
        // SAFETY: Non-null handles were created by this device and are uniquely
        // owned. Successful submission transfers these handles to the deferred
        // owner before this guard drops; failure paths never submitted them.
        unsafe {
            if self.fence != vk::Fence::null() {
                self.context.device.destroy_fence(self.fence, None);
            }
            if self.command_pool != vk::CommandPool::null() {
                self.context
                    .device
                    .destroy_command_pool(self.command_pool, None);
            }
        }
        self.staging.destroy(self.context.allocator);
    }
}

/// Submits shared M2 geometry and retains staging for non-blocking retirement.
pub(super) fn upload_mesh(
    context: MeshUploadContext<'_>,
    plan: &M2MeshPlan,
) -> Result<(GpuM2Mesh, DeferredMeshTransfer), VulkanError> {
    let vertex_bytes = plan.vertex_bytes();
    let index_bytes = plan.index_bytes();
    if vertex_bytes.is_empty() {
        return Err(VulkanError::EmptyM2Mesh {
            path: plan.path().clone(),
            buffer_kind: "vertex",
        });
    }
    if index_bytes.is_empty() {
        return Err(VulkanError::EmptyM2Mesh {
            path: plan.path().clone(),
            buffer_kind: "index",
        });
    }
    let info = M2MeshResourceInfo::new(
        plan.path().clone(),
        plan.profile_index(),
        plan.vertices().len(),
        plan.indices().len(),
        vertex_bytes.len(),
        index_bytes.len(),
        plan.max_bone_index(),
    );
    let (buffers, transfer) = upload_mesh_buffers_deferred(context, &vertex_bytes, &index_bytes)?;
    Ok((GpuM2Mesh { buffers, info }, transfer))
}

/// Uploads one nonempty serialized vertex/index pair for any typed mesh owner.
pub(in crate::device) fn upload_mesh_buffers(
    context: MeshUploadContext<'_>,
    vertex_bytes: &[u8],
    index_bytes: &[u8],
) -> Result<GpuMeshBuffers, VulkanError> {
    let (mut buffers, mut transfer) =
        upload_mesh_buffers_deferred(context, vertex_bytes, index_bytes)?;
    let result = transfer.wait(context.device);
    transfer.destroy(context.device, context.allocator);
    if let Err(error) = result {
        buffers.destroy(context.allocator);
        return Err(error);
    }
    Ok(buffers)
}

/// Records transfer-to-input barriers before later draws on the graphics queue.
///
/// Vulkan's submission-order dependency extends to subsequent submissions on
/// this same queue. The fence controls staging lifetime, not draw readiness.
fn upload_mesh_buffers_deferred(
    context: MeshUploadContext<'_>,
    vertex_bytes: &[u8],
    index_bytes: &[u8],
) -> Result<(GpuMeshBuffers, DeferredMeshTransfer), VulkanError> {
    // Vulkan buffer copies operate in four-byte units. Preserve logical byte
    // lengths in diagnostics while padding only transfer/allocation storage.
    let vertex_copy_size = aligned_copy_size(vertex_bytes.len())?;
    let index_copy_size = aligned_copy_size(index_bytes.len())?;
    let staging_size = vertex_copy_size
        .checked_add(index_copy_size)
        .ok_or_else(|| VulkanError::operation("size M2 staging buffer", "size overflow"))?;
    let vertex_device_size = u64::try_from(vertex_copy_size)
        .map_err(|source| VulkanError::operation("convert M2 vertex buffer size", source))?;
    let index_device_size = u64::try_from(index_copy_size)
        .map_err(|source| VulkanError::operation("convert M2 index buffer size", source))?;
    let mut staging_bytes = Vec::with_capacity(staging_size);
    staging_bytes.extend_from_slice(vertex_bytes);
    staging_bytes.resize(vertex_copy_size, 0);
    staging_bytes.extend_from_slice(index_bytes);
    staging_bytes.resize(staging_size, 0);

    let device_allocation = vk_mem::AllocationCreateInfo {
        usage: vk_mem::MemoryUsage::AutoPreferDevice,
        required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
        ..Default::default()
    };
    let vertex_buffer = AllocatedBuffer::create(
        context.allocator,
        vertex_device_size,
        vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::VERTEX_BUFFER,
        &device_allocation,
        "create M2 vertex buffer",
    )?;
    let index_buffer = match AllocatedBuffer::create(
        context.allocator,
        index_device_size,
        vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::INDEX_BUFFER,
        &device_allocation,
        "create M2 index buffer",
    ) {
        Ok(buffer) => buffer,
        Err(error) => {
            let mut vertex_buffer = vertex_buffer;
            vertex_buffer.destroy(context.allocator);
            return Err(error);
        }
    };
    let guard = MeshGuard {
        allocator: context.allocator,
        buffers: Some(GpuMeshBuffers {
            vertex_buffer,
            index_buffer,
        }),
    };
    let transfer = TransferResources::create(context, &staging_bytes)?;
    let command_buffer = transfer.command_buffer()?;
    let buffers = guard.buffers.as_ref().ok_or_else(|| {
        VulkanError::operation("record mesh upload", "mesh buffers are unavailable")
    })?;
    record_copies(
        context.device,
        command_buffer,
        transfer.staging.handle,
        buffers.vertex_buffer.handle,
        buffers.index_buffer.handle,
        vertex_device_size,
        index_device_size,
    )?;
    let mut buffers = guard.finish()?;
    match transfer.submit_and_defer(command_buffer) {
        Ok(transfer) => Ok((buffers, transfer)),
        Err(error) => {
            buffers.destroy(context.allocator);
            Err(error)
        }
    }
}

/// Rounds a nonempty logical buffer length to Vulkan's four-byte copy unit.
fn aligned_copy_size(size: usize) -> Result<usize, VulkanError> {
    size.checked_add(3)
        .map(|value| value & !3)
        .ok_or_else(|| VulkanError::operation("align M2 buffer size", "size overflow"))
}

/// Records both device-local copies and their transfer-to-input memory barrier.
#[allow(clippy::too_many_arguments)]
fn record_copies(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    staging_buffer: vk::Buffer,
    vertex_buffer: vk::Buffer,
    index_buffer: vk::Buffer,
    vertex_size: vk::DeviceSize,
    index_size: vk::DeviceSize,
) -> Result<(), VulkanError> {
    let begin_info =
        vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
    // SAFETY: The primary command buffer is newly allocated and not pending.
    unsafe { device.begin_command_buffer(command_buffer, &begin_info) }
        .map_err(|source| VulkanError::operation("begin M2 transfer command buffer", source))?;
    let vertex_copy = vk::BufferCopy::default().size(vertex_size);
    let index_copy = vk::BufferCopy::default()
        .src_offset(vertex_size)
        .size(index_size);
    // SAFETY: Source/destination offsets and sizes are four-byte aligned and
    // remain within the allocated buffers for the command's lifetime.
    unsafe {
        device.cmd_copy_buffer(
            command_buffer,
            staging_buffer,
            vertex_buffer,
            &[vertex_copy],
        );
        device.cmd_copy_buffer(command_buffer, staging_buffer, index_buffer, &[index_copy]);
    }

    let vertex_barrier = vk::BufferMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
        .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::VERTEX_INPUT)
        .dst_access_mask(vk::AccessFlags2::VERTEX_ATTRIBUTE_READ)
        .buffer(vertex_buffer)
        .offset(0)
        .size(vk::WHOLE_SIZE);
    let index_barrier = vk::BufferMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
        .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::INDEX_INPUT)
        .dst_access_mask(vk::AccessFlags2::INDEX_READ)
        .buffer(index_buffer)
        .offset(0)
        .size(vk::WHOLE_SIZE);
    let barriers = [vertex_barrier, index_barrier];
    let dependency = vk::DependencyInfo::default().buffer_memory_barriers(&barriers);
    // SAFETY: Synchronization2 is enabled and both transfer writes precede any
    // future vertex/index reads on this graphics queue.
    unsafe { device.cmd_pipeline_barrier2(command_buffer, &dependency) };
    // SAFETY: All recorded objects remain live through submission retirement.
    unsafe { device.end_command_buffer(command_buffer) }
        .map_err(|source| VulkanError::operation("end M2 transfer command buffer", source))
}
