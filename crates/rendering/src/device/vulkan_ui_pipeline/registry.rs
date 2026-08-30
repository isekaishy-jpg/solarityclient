//! Variant deduplication and renderer-lifetime UI pipeline ownership.

#![allow(unsafe_code)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use ash::{Device, vk};

use crate::device::VulkanError;
use crate::{UiRenderBlend, UiShaderSource, UiSpirvCompiler};

use super::pipeline::{UiPipelineLayout, create_pipeline};
use super::{UiPipelineHandle, UiPipelineInfo};

/// One driver pipeline and its complete simple-render identity.
struct GpuUiPipeline {
    handle: vk::Pipeline,
    info: UiPipelineInfo,
}

/// Owns the common layout and at most four UI pipeline variants.
pub(in crate::device) struct UiPipelineRegistry {
    registry_id: u64,
    compiler: Option<UiSpirvCompiler>,
    layout: UiPipelineLayout,
    handles: HashMap<UiPipelineInfo, UiPipelineHandle>,
    resources: Vec<GpuUiPipeline>,
}

impl Default for UiPipelineRegistry {
    /// Assigns process-unique locality without creating Vulkan objects.
    fn default() -> Self {
        static NEXT_REGISTRY_ID: AtomicU64 = AtomicU64::new(1);

        Self {
            registry_id: NEXT_REGISTRY_ID.fetch_add(1, Ordering::Relaxed),
            compiler: None,
            layout: UiPipelineLayout::default(),
            handles: HashMap::with_capacity(4),
            resources: Vec::with_capacity(4),
        }
    }
}

impl UiPipelineRegistry {
    /// Returns the initialized sampled-image descriptor layout.
    pub(in crate::device) fn texture_set_layout(
        &mut self,
        device: &Device,
    ) -> Result<vk::DescriptorSetLayout, VulkanError> {
        self.layout.ensure_created(device)?;
        Ok(self.layout.texture_set())
    }

    /// Returns an existing pipeline or compiles one exact source/blend variant.
    pub(in crate::device) fn prepare(
        &mut self,
        device: &Device,
        color_format: vk::Format,
        source: UiShaderSource,
        blend: UiRenderBlend,
    ) -> Result<UiPipelineHandle, VulkanError> {
        let info = UiPipelineInfo::new(source, blend);
        if let Some(handle) = self.handles.get(&info) {
            return Ok(*handle);
        }
        let slot = u32::try_from(self.resources.len())
            .map_err(|_source| VulkanError::UiPipelineCapacity)?;
        self.layout.ensure_created(device)?;
        let compiler = match self.compiler.as_ref() {
            Some(compiler) => compiler,
            None => {
                self.compiler = Some(UiSpirvCompiler::new().map_err(shader_error)?);
                self.compiler.as_ref().ok_or_else(|| {
                    VulkanError::operation("access UI shader compiler", "compiler is unavailable")
                })?
            }
        };
        let program = compiler.compile(source).map_err(shader_error)?;
        let pipeline =
            create_pipeline(device, self.layout.handle(), color_format, blend, &program)?;
        let handle = UiPipelineHandle {
            registry_id: self.registry_id,
            slot,
        };
        self.resources.push(GpuUiPipeline {
            handle: pipeline,
            info,
        });
        self.handles.insert(info, handle);
        Ok(handle)
    }

    /// Returns immutable diagnostics for a live UI pipeline.
    pub(in crate::device) fn info(&self, handle: UiPipelineHandle) -> Option<UiPipelineInfo> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .map(|resource| resource.info)
    }

    /// Resolves one renderer-local pipeline to command-recording handles.
    pub(in crate::device) fn raw(
        &self,
        handle: UiPipelineHandle,
    ) -> Option<(vk::Pipeline, vk::PipelineLayout)> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .map(|resource| (resource.handle, self.layout.handle()))
    }

    /// Destroys pipelines before their common layout and descriptor layout.
    pub(in crate::device) fn destroy(&mut self, device: &Device) {
        self.handles.clear();
        // SAFETY: Each pipeline belongs to this device and submitted work is idle.
        unsafe {
            for resource in self.resources.drain(..).rev() {
                device.destroy_pipeline(resource.handle, None);
            }
        }
        self.layout.destroy(device);
    }
}

/// Erases shaderc's implementation type at the device facade boundary.
fn shader_error(error: impl ToString) -> VulkanError {
    VulkanError::UiShader {
        message: error.to_string(),
    }
}
