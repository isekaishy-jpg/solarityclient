//! Material deduplication and renderer-lifetime ribbon pipeline ownership.

#![allow(unsafe_code)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use ash::{Device, vk};

use crate::device::VulkanError;
use crate::{M2MaterialState, M2RibbonSpirvCompiler, M2RibbonSpirvProgram};

use super::pipeline::{M2RibbonPipelineLayout, create_pipeline};
use super::{M2RibbonPipelineHandle, M2RibbonPipelineInfo};

struct GpuM2RibbonPipeline {
    handle: vk::Pipeline,
    info: M2RibbonPipelineInfo,
}

pub(in crate::device) struct M2RibbonPipelineRegistry {
    registry_id: u64,
    compiler: Option<M2RibbonSpirvCompiler>,
    layout: M2RibbonPipelineLayout,
    handles: HashMap<M2MaterialState, M2RibbonPipelineHandle>,
    resources: Vec<GpuM2RibbonPipeline>,
}

impl Default for M2RibbonPipelineRegistry {
    fn default() -> Self {
        static NEXT_REGISTRY_ID: AtomicU64 = AtomicU64::new(1);

        Self {
            registry_id: NEXT_REGISTRY_ID.fetch_add(1, Ordering::Relaxed),
            compiler: None,
            layout: M2RibbonPipelineLayout::default(),
            handles: HashMap::new(),
            resources: Vec::new(),
        }
    }
}

impl M2RibbonPipelineRegistry {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::device) fn prepare(
        &mut self,
        device: &Device,
        pipeline_cache: vk::PipelineCache,
        color_format: vk::Format,
        depth_format: vk::Format,
        scene_set: vk::DescriptorSetLayout,
        texture_set: vk::DescriptorSetLayout,
        material: M2MaterialState,
    ) -> Result<M2RibbonPipelineHandle, VulkanError> {
        if let Some(handle) = self.handles.get(&material) {
            return Ok(*handle);
        }
        let compiler = match self.compiler.as_ref() {
            Some(compiler) => compiler,
            None => {
                self.compiler = Some(M2RibbonSpirvCompiler::new().map_err(shader_error)?);
                self.compiler.as_ref().ok_or_else(|| {
                    VulkanError::operation(
                        "access M2 ribbon shader compiler",
                        "compiler is unavailable",
                    )
                })?
            }
        };
        let program = compiler.compile(material).map_err(shader_error)?;
        self.prepare_precompiled(
            device,
            pipeline_cache,
            color_format,
            depth_format,
            scene_set,
            texture_set,
            &program,
        )
    }

    /// Creates one driver pipeline from worker-compiled ribbon bytecode.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::device) fn prepare_precompiled(
        &mut self,
        device: &Device,
        pipeline_cache: vk::PipelineCache,
        color_format: vk::Format,
        depth_format: vk::Format,
        scene_set: vk::DescriptorSetLayout,
        texture_set: vk::DescriptorSetLayout,
        program: &M2RibbonSpirvProgram,
    ) -> Result<M2RibbonPipelineHandle, VulkanError> {
        let material = program.material();
        if let Some(handle) = self.handles.get(&material) {
            return Ok(*handle);
        }
        let slot = u32::try_from(self.resources.len())
            .map_err(|_source| VulkanError::M2RibbonPipelineCapacity)?;
        self.layout.ensure_created(device, scene_set, texture_set)?;
        let pipeline = create_pipeline(
            device,
            pipeline_cache,
            self.layout.handle(),
            color_format,
            depth_format,
            material,
            program,
        )?;
        let handle = M2RibbonPipelineHandle {
            registry_id: self.registry_id,
            slot,
        };
        self.resources.push(GpuM2RibbonPipeline {
            handle: pipeline,
            info: M2RibbonPipelineInfo::new(material),
        });
        self.handles.insert(material, handle);
        Ok(handle)
    }

    pub(in crate::device) fn info(
        &self,
        handle: M2RibbonPipelineHandle,
    ) -> Option<M2RibbonPipelineInfo> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .map(|resource| resource.info)
    }

    pub(in crate::device) fn raw(
        &self,
        handle: M2RibbonPipelineHandle,
    ) -> Option<(vk::Pipeline, vk::PipelineLayout)> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .map(|resource| (resource.handle, self.layout.handle()))
    }

    pub(in crate::device) fn destroy(&mut self, device: &Device) {
        self.handles.clear();
        // SAFETY: Device work is idle and every pipeline is uniquely owned.
        unsafe {
            for resource in self.resources.drain(..).rev() {
                device.destroy_pipeline(resource.handle, None);
            }
        }
        self.layout.destroy(device);
    }
}

fn shader_error(error: impl ToString) -> VulkanError {
    VulkanError::M2RibbonShader {
        message: error.to_string(),
    }
}
