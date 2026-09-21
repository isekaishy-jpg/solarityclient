//! Graphics bindings retained only within one world command buffer recording.

use ash::vk;

use super::super::recording::scene::{SceneBindings, SceneCommand, SceneRecording};
use super::RecordContext;

/// All typed world streams share binding zero. Buffer offsets and index widths
/// are part of the binding identity, including transitions to particle streams.
/// Descriptor sets remain bound per draw with their current layout and offsets.
#[derive(Default)]
pub(super) struct WorldCommandBindings<'a> {
    capture: Option<&'a mut SceneRecording>,
    pub(super) fog: super::super::fog::SubmissionFog,
    pipeline: vk::Pipeline,
    vertex: Option<(vk::Buffer, vk::DeviceSize)>,
    index: Option<(vk::Buffer, vk::DeviceSize, vk::IndexType)>,
}

impl<'a> WorldCommandBindings<'a> {
    pub(super) fn with_fog(fog: super::super::fog::SubmissionFog) -> Self {
        Self {
            fog,
            ..Self::default()
        }
    }

    /// Captures numeric commands after the ordered sky prefix has established fog.
    pub(super) fn capturing(
        fog: super::super::fog::SubmissionFog,
        capture: &'a mut SceneRecording,
    ) -> Self {
        Self {
            fog,
            capture: Some(capture),
            ..Self::default()
        }
    }
    /// Sky uses direct recording; the central scene records through owned worker ranges.
    pub(super) fn draw(
        &mut self,
        context: &RecordContext<'_>,
        draw: SceneCommand,
    ) -> Result<(), crate::VulkanError> {
        if let Some(capture) = &mut self.capture {
            return capture.push(draw);
        }
        SceneBindings::default().record(context.device, context.command_buffer, &draw);
        // A direct complete command may have changed every cached binding.
        self.pipeline = vk::Pipeline::null();
        self.vertex = None;
        self.index = None;
        Ok(())
    }
    /// Timestamp commands follow exactly the same GPU order as their original draws.
    pub(super) fn timestamp(
        &mut self,
        context: &RecordContext<'_>,
        index: u32,
    ) -> Result<(), crate::VulkanError> {
        if let Some(pool) = context.gpu_queries {
            self.draw(
                context,
                SceneCommand {
                    timestamp: Some((pool, index)),
                    ..Default::default()
                },
            )?;
        }
        Ok(())
    }

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
