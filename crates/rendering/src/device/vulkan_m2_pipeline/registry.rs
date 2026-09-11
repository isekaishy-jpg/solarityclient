//! Effect deduplication and renderer-lifetime pipeline ownership.

#![allow(unsafe_code)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use ash::{Device, vk};

use crate::device::{M2ModelOrientation, VulkanError};
use crate::shader::{
    M2ShaderPermutation, M2ShaderPlan, M2SpirvCompiler, M2SpirvKey, M2SpirvProgram,
};

use super::pipeline::{M2PipelineLayout, create_pipeline};
use super::types::{M2PipelineHandle, M2PipelineInfo};

/// One driver pipeline and its stock-facing diagnostic identity.
struct GpuM2Pipeline {
    handle: vk::Pipeline,
    primary_shadow: vk::Pipeline,
    info: M2PipelineInfo,
}

/// Owns the shared layout and every compiled M2 pipeline for one renderer.
pub(in crate::device) struct M2PipelineRegistry {
    registry_id: u64,
    compiler: Option<M2SpirvCompiler>,
    layout: M2PipelineLayout,
    handles: HashMap<(M2SpirvKey, M2ModelOrientation), M2PipelineHandle>,
    resources: Vec<GpuM2Pipeline>,
    liquid_clipping: bool,
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
            liquid_clipping: false,
        }
    }
}

impl M2PipelineRegistry {
    /// The adapter's clip-distance support is fixed before any pipeline exists.
    pub(in crate::device) fn with_liquid_clipping(enabled: bool) -> Self {
        Self {
            liquid_clipping: enabled,
            ..Self::default()
        }
    }

    pub(in crate::device) const fn liquid_clipping_enabled(&self) -> bool {
        self.liquid_clipping
    }

    /// Borrows the existing four-sampler receiver ABI after layout creation.
    pub(in crate::device) fn shadow_set_layout(&self) -> Option<vk::DescriptorSetLayout> {
        self.layout.descriptor_set(4)
    }

    /// Resolves the paired primary receiver without changing prepared packet identity.
    pub(in crate::device) fn raw_primary_shadow(
        &self,
        handle: M2PipelineHandle,
    ) -> Option<(vk::Pipeline, vk::PipelineLayout)> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .filter(|resource| resource.primary_shadow != vk::Pipeline::null())
            .map(|resource| (resource.primary_shadow, self.layout.handle()))
    }
    /// Borrows the existing bone, material, and texture ABI for shadow casters.
    pub(in crate::device) fn caster_set_layouts(&self) -> Option<[vk::DescriptorSetLayout; 3]> {
        Some([
            self.layout.descriptor_set(1)?,
            self.layout.descriptor_set(2)?,
            self.layout.descriptor_set(3)?,
        ])
    }
    /// Returns the initialized scene, bone, and material layouts in set order.
    pub(in crate::device) fn frame_set_layouts(
        &mut self,
        device: &Device,
    ) -> Result<[vk::DescriptorSetLayout; 3], VulkanError> {
        self.layout.ensure_created(device)?;
        let mut layouts = [vk::DescriptorSetLayout::null(); 3];
        for (index, layout) in layouts.iter_mut().enumerate() {
            *layout = self.layout.descriptor_set(index).ok_or_else(|| {
                VulkanError::operation("access M2 frame descriptor layout", "layout is unavailable")
            })?;
        }
        Ok(layouts)
    }

    /// Resolves one renderer-local pipeline to command-recording handles.
    pub(in crate::device) fn raw(
        &self,
        handle: M2PipelineHandle,
    ) -> Option<(vk::Pipeline, vk::PipelineLayout)> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .map(|resource| (resource.handle, self.layout.handle()))
    }

    /// Returns the initialized set-three sampled-texture layout.
    pub(in crate::device) fn texture_set_layout(
        &mut self,
        device: &Device,
    ) -> Result<vk::DescriptorSetLayout, VulkanError> {
        const TEXTURE_SET_INDEX: usize = 3;

        self.layout.ensure_created(device)?;
        self.layout
            .descriptor_set(TEXTURE_SET_INDEX)
            .ok_or_else(|| {
                VulkanError::operation(
                    "access M2 texture descriptor layout",
                    "layout is unavailable",
                )
            })
    }

    /// Returns an existing pipeline or compiles and creates one exact variant.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::device) fn prepare(
        &mut self,
        device: &Device,
        pipeline_cache: vk::PipelineCache,
        color_format: vk::Format,
        depth_format: vk::Format,
        plan: M2ShaderPlan,
        permutation: M2ShaderPermutation,
        orientation: M2ModelOrientation,
    ) -> Result<M2PipelineHandle, VulkanError> {
        let key = M2SpirvKey::new(plan, permutation);
        if let Some(handle) = self.handles.get(&(key, orientation)) {
            return Ok(*handle);
        }
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
        self.prepare_precompiled(
            device,
            pipeline_cache,
            color_format,
            depth_format,
            &program,
            orientation,
        )
    }

    /// Creates a pipeline from worker-compiled bytecode after identity validation.
    pub(in crate::device) fn prepare_precompiled(
        &mut self,
        device: &Device,
        pipeline_cache: vk::PipelineCache,
        color_format: vk::Format,
        depth_format: vk::Format,
        program: &M2SpirvProgram,
        orientation: M2ModelOrientation,
    ) -> Result<M2PipelineHandle, VulkanError> {
        let key = program.key();
        if let Some(handle) = self.handles.get(&(key, orientation)) {
            return Ok(*handle);
        }
        let plan = key.plan();
        let permutation = key.permutation();
        let slot = u32::try_from(self.resources.len())
            .map_err(|_source| VulkanError::M2PipelineCapacity)?;
        self.layout.ensure_created(device)?;
        let pipeline = create_pipeline(
            device,
            pipeline_cache,
            self.layout.handle(),
            color_format,
            depth_format,
            plan.material(),
            program,
            orientation,
            self.liquid_clipping,
        )?;
        let handle = M2PipelineHandle {
            registry_id: self.registry_id,
            slot,
        };
        let primary_shadow = if permutation.has_shadows() {
            vk::Pipeline::null()
        } else {
            // Build alongside the source pipeline before publication, sharing
            // immutable compiled modules with the ordinary material variant.
            let result = (|| {
                let program = M2SpirvCompiler::new()
                    .map_err(shader_error)?
                    .compile(plan, permutation.with_primary_shadow())
                    .map_err(shader_error)?;
                create_pipeline(
                    device,
                    pipeline_cache,
                    self.layout.handle(),
                    color_format,
                    depth_format,
                    plan.material(),
                    &program,
                    orientation,
                    self.liquid_clipping,
                )
            })();
            match result {
                Ok(pipeline) => pipeline,
                Err(error) => {
                    // SAFETY: The unpublished base pipeline has never entered a command buffer.
                    unsafe {
                        device.destroy_pipeline(pipeline, None);
                    }
                    return Err(error);
                }
            }
        };
        self.resources.push(GpuM2Pipeline {
            handle: pipeline,
            primary_shadow,
            info: M2PipelineInfo::new(
                plan.vertex_shader(),
                plan.pixel_shader(),
                plan.texture_count(),
                plan.material(),
                permutation,
            ),
        });
        self.handles.insert((key, orientation), handle);
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
                if resource.primary_shadow != vk::Pipeline::null() {
                    device.destroy_pipeline(resource.primary_shadow, None);
                }
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
