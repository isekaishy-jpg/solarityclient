//! Independent mesh allocations sharing one staging transfer and completion fence.

use ash::vk;

use super::{
    AllocatedBuffer, DeferredMeshTransfer, GpuMeshBuffers, MeshUploadBytes, MeshUploadContext,
    TransferResources, VulkanError,
};

/// Validates and submits an entire group; failure publishes no mesh allocations.
pub(in crate::device) fn upload_mesh_batch_deferred(
    context: MeshUploadContext<'_>,
    payloads: &[(&[u8], &[u8])],
) -> Result<(Vec<GpuMeshBuffers>, DeferredMeshTransfer), VulkanError> {
    let profile = std::env::var_os("SOLARITY_FRAME_TIMINGS").map(|_| std::time::Instant::now());
    if payloads.is_empty()
        || payloads
            .iter()
            .any(|(vertices, indices)| vertices.is_empty() || indices.is_empty())
    {
        return Err(VulkanError::operation(
            "upload mesh batch",
            "empty geometry",
        ));
    }
    let layouts = payloads
        .iter()
        .map(|&(vertices, indices)| MeshUploadBytes::new(vertices, indices))
        .collect::<Result<Vec<_>, _>>()?;
    let vertex_capacity = layouts
        .iter()
        .try_fold(0usize, |size, layout| {
            size.checked_add(layout.vertex_copy_size)
        })
        .ok_or_else(|| VulkanError::operation("size mesh batch vertices", "size overflow"))?;
    let index_capacity = layouts
        .iter()
        .try_fold(0usize, |size, layout| {
            size.checked_add(layout.index_copy_size)
        })
        .ok_or_else(|| VulkanError::operation("size mesh batch indices", "size overflow"))?;
    vertex_capacity
        .checked_add(index_capacity)
        .ok_or_else(|| VulkanError::operation("size mesh batch staging", "size overflow"))?;
    let mut vertices = Vec::with_capacity(vertex_capacity);
    let mut indices = Vec::with_capacity(index_capacity);
    let mut ranges = Vec::with_capacity(layouts.len());
    for layout in &layouts {
        let vertex_offset = vertices.len();
        let index_offset = indices.len();
        append_padded(&mut vertices, layout.vertices, layout.vertex_copy_size)?;
        append_padded(&mut indices, layout.indices, layout.index_copy_size)?;
        ranges.push((vertex_offset as u64, index_offset as u64));
    }
    let vertex_extent = vertices.len() as u64;
    let allocation = vk_mem::AllocationCreateInfo {
        usage: vk_mem::MemoryUsage::AutoPreferDevice,
        required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
        ..Default::default()
    };
    let mut guard = BatchGuard {
        allocator: context.allocator,
        buffers: Vec::with_capacity(layouts.len()),
    };
    for layout in &layouts {
        let mut vertex_buffer = AllocatedBuffer::create(
            context.allocator,
            layout.vertex_copy_size as u64,
            vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::VERTEX_BUFFER,
            &allocation,
            "create batch vertex buffer",
        )?;
        let index_buffer = match AllocatedBuffer::create(
            context.allocator,
            layout.index_copy_size as u64,
            vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::INDEX_BUFFER,
            &allocation,
            "create batch index buffer",
        ) {
            Ok(buffer) => buffer,
            Err(error) => {
                vertex_buffer.destroy(context.allocator);
                return Err(error);
            }
        };
        guard.buffers.push(GpuMeshBuffers {
            vertex_buffer,
            index_buffer,
        });
    }
    let transfer = TransferResources::create(context, MeshUploadBytes::new(&vertices, &indices)?)?;
    let command = transfer.command_buffer()?;
    let begin =
        vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
    // SAFETY: This primary command buffer is newly allocated and not pending.
    unsafe { context.device.begin_command_buffer(command, &begin) }
        .map_err(|error| VulkanError::operation("begin mesh batch transfer", error))?;
    let mut barriers = Vec::with_capacity(layouts.len() * 2);
    for ((buffers, layout), &(vertex_offset, index_offset)) in
        guard.buffers.iter().zip(&layouts).zip(&ranges)
    {
        let vertex_copy = vk::BufferCopy::default()
            .src_offset(vertex_offset)
            .size(layout.vertex_copy_size as u64);
        let index_copy = vk::BufferCopy::default()
            .src_offset(vertex_extent + index_offset)
            .size(layout.index_copy_size as u64);
        // SAFETY: Each independently padded source range is initialized and
        // four-byte aligned; each destination allocation covers its copy.
        unsafe {
            context.device.cmd_copy_buffer(
                command,
                transfer.staging.handle,
                buffers.vertex_buffer.handle,
                &[vertex_copy],
            );
            context.device.cmd_copy_buffer(
                command,
                transfer.staging.handle,
                buffers.index_buffer.handle,
                &[index_copy],
            );
        }
        for (buffer, stage, access) in [
            (
                buffers.vertex_buffer.handle,
                vk::PipelineStageFlags2::VERTEX_INPUT,
                vk::AccessFlags2::VERTEX_ATTRIBUTE_READ,
            ),
            (
                buffers.index_buffer.handle,
                vk::PipelineStageFlags2::INDEX_INPUT,
                vk::AccessFlags2::INDEX_READ,
            ),
        ] {
            barriers.push(
                vk::BufferMemoryBarrier2::default()
                    .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
                    .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                    .dst_stage_mask(stage)
                    .dst_access_mask(access)
                    .buffer(buffer)
                    .offset(0)
                    .size(vk::WHOLE_SIZE),
            );
        }
    }
    let dependency = vk::DependencyInfo::default().buffer_memory_barriers(&barriers);
    // SAFETY: All copied buffers remain owned by the guard. These barriers
    // cover later input reads on the same graphics queue, including later submits.
    unsafe {
        context.device.cmd_pipeline_barrier2(command, &dependency);
        context.device.end_command_buffer(command)
    }
    .map_err(|error| VulkanError::operation("end mesh batch transfer", error))?;
    let transfer = transfer.submit_and_defer(command)?;
    // Leave the pre-submission cleanup guard before optional tracing can run.
    let buffers = std::mem::take(&mut guard.buffers);
    if let Some(start) = profile {
        tracing::info!(
            target: "solarity_rendering::device::mesh_upload",
            meshes = payloads.len(),
            vertex_bytes = vertex_capacity,
            index_bytes = index_capacity,
            total_us = start.elapsed().as_secs_f64() * 1_000_000.0,
            "profiled mesh batch admission"
        );
    }
    // Successful submission transfers both resource lifetimes to the caller.
    Ok((buffers, transfer))
}

fn append_padded(output: &mut Vec<u8>, payload: &[u8], padded: usize) -> Result<(), VulkanError> {
    let end = output
        .len()
        .checked_add(padded)
        .ok_or_else(|| VulkanError::operation("size mesh batch staging", "size overflow"))?;
    output.extend_from_slice(payload);
    output.resize(end, 0);
    Ok(())
}

struct BatchGuard<'a> {
    allocator: &'a vk_mem::Allocator,
    buffers: Vec<GpuMeshBuffers>,
}

impl Drop for BatchGuard<'_> {
    fn drop(&mut self) {
        for buffer in &mut self.buffers {
            buffer.destroy(self.allocator);
        }
    }
}
