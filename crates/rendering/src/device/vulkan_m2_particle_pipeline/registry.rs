//! Material deduplication and renderer-lifetime particle pipeline ownership.

#![allow(unsafe_code)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use ash::{Device, vk};

use crate::device::VulkanError;
use crate::{M2MaterialState, M2ParticleSpirvCompiler, M2ParticleSpirvProgram};

use super::pipeline::{M2ParticlePipelineLayout, create_pipeline};
use super::{M2ParticlePipelineHandle, M2ParticlePipelineInfo};

struct GpuM2ParticlePipeline {
    handle: vk::Pipeline,
    info: M2ParticlePipelineInfo,
}

pub(in crate::device) struct M2ParticlePipelineRegistry {
    registry_id: u64,
    compiler: Option<M2ParticleSpirvCompiler>,
    layout: M2ParticlePipelineLayout,
    handles: HashMap<M2MaterialState, M2ParticlePipelineHandle>,
    resources: Vec<GpuM2ParticlePipeline>,
}

impl Default for M2ParticlePipelineRegistry {
    fn default() -> Self {
        static NEXT_REGISTRY_ID: AtomicU64 = AtomicU64::new(1);

        Self {
            registry_id: NEXT_REGISTRY_ID.fetch_add(1, Ordering::Relaxed),
            compiler: None,
            layout: M2ParticlePipelineLayout::default(),
            handles: HashMap::new(),
            resources: Vec::new(),
        }
    }
}

impl M2ParticlePipelineRegistry {
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
    ) -> Result<M2ParticlePipelineHandle, VulkanError> {
        if let Some(handle) = self.handles.get(&material) {
            return Ok(*handle);
        }
        let compiler = match self.compiler.as_ref() {
            Some(compiler) => compiler,
            None => {
                self.compiler = Some(M2ParticleSpirvCompiler::new().map_err(shader_error)?);
                self.compiler.as_ref().ok_or_else(|| {
                    VulkanError::operation(
                        "access M2 particle shader compiler",
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

    /// Creates one driver pipeline from worker-compiled particle bytecode.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::device) fn prepare_precompiled(
        &mut self,
        device: &Device,
        pipeline_cache: vk::PipelineCache,
        color_format: vk::Format,
        depth_format: vk::Format,
        scene_set: vk::DescriptorSetLayout,
        texture_set: vk::DescriptorSetLayout,
        program: &M2ParticleSpirvProgram,
    ) -> Result<M2ParticlePipelineHandle, VulkanError> {
        let material = program.material();
        if let Some(handle) = self.handles.get(&material) {
            return Ok(*handle);
        }
        let slot = u32::try_from(self.resources.len())
            .map_err(|_source| VulkanError::M2ParticlePipelineCapacity)?;
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
        let handle = M2ParticlePipelineHandle {
            registry_id: self.registry_id,
            slot,
        };
        self.resources.push(GpuM2ParticlePipeline {
            handle: pipeline,
            info: M2ParticlePipelineInfo::new(material),
        });
        self.handles.insert(material, handle);
        Ok(handle)
    }

    pub(in crate::device) fn info(
        &self,
        handle: M2ParticlePipelineHandle,
    ) -> Option<M2ParticlePipelineInfo> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .map(|resource| resource.info)
    }

    pub(in crate::device) fn raw(
        &self,
        handle: M2ParticlePipelineHandle,
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
    VulkanError::M2ParticleShader {
        message: error.to_string(),
    }
}
