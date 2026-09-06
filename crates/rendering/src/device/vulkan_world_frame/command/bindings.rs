//! Graphics bindings retained only within one world command buffer recording.

use ash::vk;

use super::RecordContext;

/// All typed world streams share binding zero. Buffer offsets and index widths
/// are part of the binding identity, including transitions to particle streams.
/// Descriptor sets remain bound per draw with their current layout and offsets.
#[derive(Default)]
pub(super) struct WorldCommandBindings {
    pipeline: vk::Pipeline,
    vertex: Option<(vk::Buffer, vk::DeviceSize)>,
    index: Option<(vk::Buffer, vk::DeviceSize, vk::IndexType)>,
}

impl WorldCommandBindings {
    pub(super) fn bind_pipeline(&mut self, context: &RecordContext<'_>, pipeline: vk::Pipeline) {
        if self.pipeline == pipeline {
            return;
        }
        // SAFETY: The caller resolved this live pipeline from the renderer
        // registry, and the world command buffer is recording graphics draws.
        unsafe {
            context.device.cmd_bind_pipeline(
                context.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline,
            );
        }
        self.pipeline = pipeline;
    }

    pub(super) fn bind_vertex(
        &mut self,
        context: &RecordContext<'_>,
        vertex: (vk::Buffer, vk::DeviceSize),
    ) {
        if self.vertex == Some(vertex) {
            return;
        }
        // SAFETY: Mesh registries or the fenced frame slot own the buffer;
        // prepared draw validation proves its stream and offset are valid.
        unsafe {
            context.device.cmd_bind_vertex_buffers(
                context.command_buffer,
                0,
                &[vertex.0],
                &[vertex.1],
            );
        }
        self.vertex = Some(vertex);
    }

    pub(super) fn bind_index(
        &mut self,
        context: &RecordContext<'_>,
        index: (vk::Buffer, vk::DeviceSize),
        index_type: vk::IndexType,
    ) {
        let binding = (index.0, index.1, index_type);
        if self.index == Some(binding) {
            return;
        }
        // SAFETY: The live mesh/slot buffer and its index type match the
        // validated prepared draw, and its offset has the required alignment.
        unsafe {
            context.device.cmd_bind_index_buffer(
                context.command_buffer,
                index.0,
                index.1,
                index_type,
            );
        }
        self.index = Some(binding);
    }
}
