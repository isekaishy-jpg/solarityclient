//! Effect deduplication and renderer-lifetime pipeline ownership.

#![allow(unsafe_code)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use ash::{Device, vk};

use crate::device::VulkanError;
use crate::shader::{M2ShaderPermutation, M2ShaderPlan, M2SpirvCompiler, M2SpirvKey};

use super::pipeline::{M2PipelineLayout, create_pipeline};
use super::types::{M2PipelineHandle, M2PipelineInfo};

/// One driver pipeline and its stock-facing diagnostic identity.
struct GpuM2Pipeline {
    handle: vk::Pipeline,
    info: M2PipelineInfo,
}

/// Owns the shared layout and every compiled M2 pipeline for one renderer.
pub(in crate::device) struct M2PipelineRegistry {
    registry_id: u64,
    compiler: Option<M2SpirvCompiler>,
    layout: M2PipelineLayout,
    handles: HashMap<M2SpirvKey, M2PipelineHandle>,
    resources: Vec<GpuM2Pipeline>,
}

impl Default for M2PipelineRegistry {
    /// Assigns process-unique locality without allocating Vulkan objects yet.
    fn default() -> Self {
        static NEXT_REGISTRY_ID: AtomicU64 = AtomicU64::new(1);

        Self {
            registry_id: NEXT_REGISTRY_ID.fetch_add(1, Ordering::Relaxed),
            compiler: None,
            layout: M2PipelineLayout::default(),
            handles: HashMap::new(),
            resources: Vec::new(),
        }
    }
}

impl M2PipelineRegistry {
    /// Returns an existing pipeline or compiles and creates one exact variant.
    pub(in crate::device) fn prepare(
        &mut self,
        device: &Device,
        color_format: vk::Format,
        depth_format: vk::Format,
        plan: M2ShaderPlan,
        permutation: M2ShaderPermutation,
    ) -> Result<M2PipelineHandle, VulkanError> {
        let key = M2SpirvKey::new(plan, permutation);
        if let Some(handle) = self.handles.get(&key) {
            return Ok(*handle);
        }
        let slot = u32::try_from(self.resources.len())
            .map_err(|_source| VulkanError::M2PipelineCapacity)?;
        self.layout.ensure_created(device)?;
        let compiler = match self.compiler.as_ref() {
            Some(compiler) => compiler,
            None => {
                self.compiler = Some(M2SpirvCompiler::new().map_err(shader_error)?);
                self.compiler.as_ref().ok_or_else(|| {
                    VulkanError::operation("access M2 shader compiler", "compiler is unavailable")
                })?
            }
        };
        let program = compiler.compile(plan, permutation).map_err(shader_error)?;
        let pipeline = create_pipeline(
            device,
            self.layout.handle(),
            color_format,
            depth_format,
            plan.material(),
            &program,
        )?;
        let handle = M2PipelineHandle {
            registry_id: self.registry_id,
            slot,
        };
        self.resources.push(GpuM2Pipeline {
            handle: pipeline,
            info: M2PipelineInfo::new(plan.vertex_shader(), plan.pixel_shader(), permutation),
        });
        self.handles.insert(key, handle);
        Ok(handle)
    }

    /// Resolves renderer-local diagnostics without exposing Vulkan handles.
    pub(in crate::device) fn info(&self, handle: M2PipelineHandle) -> Option<&M2PipelineInfo> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .map(|resource| &resource.info)
    }

    /// Destroys pipelines before their common layout and descriptor layouts.
    pub(in crate::device) fn destroy(&mut self, device: &Device) {
        self.handles.clear();
        // SAFETY: Each pipeline was created by this device, is no longer in
        // flight after renderer idle, and is destroyed exactly once.
        unsafe {
            for resource in self.resources.drain(..).rev() {
                device.destroy_pipeline(resource.handle, None);
            }
        }
        self.layout.destroy(device);
    }
}

/// Erases the compiler's implementation type at the device facade boundary.
fn shader_error(error: impl ToString) -> VulkanError {
    VulkanError::M2Shader {
        message: error.to_string(),
    }
}
