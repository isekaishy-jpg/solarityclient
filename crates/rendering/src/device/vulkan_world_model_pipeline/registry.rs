//! Effect/material deduplication and renderer-lifetime pipeline ownership.

#![allow(unsafe_code)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use ash::{Device, vk};

use crate::device::VulkanError;
use crate::{WorldModelSpirvCompiler, WorldModelSpirvKey, WorldModelSurfacePass};

use super::pipeline::{WorldModelPipelineLayout, create_pipeline};
use super::{WorldModelPipelineHandle, WorldModelPipelineInfo};

struct GpuWorldModelPipeline {
    handle: vk::Pipeline,
    info: WorldModelPipelineInfo,
}

pub(in crate::device) struct WorldModelPipelineRegistry {
    registry_id: u64,
    compiler: Option<WorldModelSpirvCompiler>,
    layout: WorldModelPipelineLayout,
    handles: HashMap<WorldModelPipelineInfo, WorldModelPipelineHandle>,
    resources: Vec<GpuWorldModelPipeline>,
}

impl Default for WorldModelPipelineRegistry {
    fn default() -> Self {
        static NEXT_REGISTRY_ID: AtomicU64 = AtomicU64::new(1);

        Self {
            registry_id: NEXT_REGISTRY_ID.fetch_add(1, Ordering::Relaxed),
            compiler: None,
            layout: WorldModelPipelineLayout::default(),
            handles: HashMap::new(),
            resources: Vec::new(),
        }
    }
}

impl WorldModelPipelineRegistry {
    pub(in crate::device) fn prepare(
        &mut self,
        device: &Device,
        color_format: vk::Format,
        depth_format: vk::Format,
        unified: bool,
        pass: WorldModelSurfacePass,
    ) -> Result<WorldModelPipelineHandle, VulkanError> {
        let effect =
            WorldModelSpirvKey::new(pass.material().shader(), unified).map_err(shader_error)?;
        let info = WorldModelPipelineInfo::new(effect, pass.material());
        if let Some(handle) = self.handles.get(&info) {
            return Ok(*handle);
        }
        let slot = u32::try_from(self.resources.len())
            .map_err(|_source| VulkanError::WorldModelPipelineCapacity)?;
        self.layout.ensure_created(device)?;
        if self.compiler.is_none() {
            self.compiler = Some(WorldModelSpirvCompiler::new().map_err(shader_error)?);
        }
        let compiler = self.compiler.as_ref().ok_or_else(|| {
            VulkanError::operation("access WMO shader compiler", "compiler is unavailable")
        })?;
        let program = compiler.compile(effect).map_err(shader_error)?;
        let pipeline = create_pipeline(
            device,
            self.layout.handle(),
            color_format,
            depth_format,
            pass.material(),
            &program,
        )?;
        let handle = WorldModelPipelineHandle {
            registry_id: self.registry_id,
            slot,
        };
        self.resources.push(GpuWorldModelPipeline {
            handle: pipeline,
            info,
        });
        self.handles.insert(info, handle);
        Ok(handle)
    }

    pub(in crate::device) fn info(
        &self,
        handle: WorldModelPipelineHandle,
    ) -> Option<WorldModelPipelineInfo> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .map(|resource| resource.info)
    }

    pub(in crate::device) fn texture_set_layout(
        &mut self,
        device: &Device,
    ) -> Result<vk::DescriptorSetLayout, VulkanError> {
        const TEXTURE_SET_INDEX: usize = 2;

        self.layout.ensure_created(device)?;
        self.layout
            .descriptor_set(TEXTURE_SET_INDEX)
            .ok_or_else(|| {
                VulkanError::operation(
                    "access WMO texture descriptor layout",
                    "layout is unavailable",
                )
            })
    }

    pub(in crate::device) fn frame_set_layouts(
        &mut self,
        device: &Device,
    ) -> Result<[vk::DescriptorSetLayout; 2], VulkanError> {
        self.layout.ensure_created(device)?;
        let mut layouts = [vk::DescriptorSetLayout::null(); 2];
        for (index, output) in layouts.iter_mut().enumerate() {
            *output = self.layout.descriptor_set(index).ok_or_else(|| {
                VulkanError::operation(
                    "access WMO frame descriptor layout",
                    "layout is unavailable",
                )
            })?;
        }
        Ok(layouts)
    }

    pub(in crate::device) fn raw(
        &self,
        handle: WorldModelPipelineHandle,
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
        // SAFETY: The renderer idles before uniquely owned pipelines are freed.
        unsafe {
            for resource in self.resources.drain(..).rev() {
                device.destroy_pipeline(resource.handle, None);
            }
        }
        self.layout.destroy(device);
    }
}

fn shader_error(error: impl ToString) -> VulkanError {
    VulkanError::WorldModelShader {
        message: error.to_string(),
    }
}
