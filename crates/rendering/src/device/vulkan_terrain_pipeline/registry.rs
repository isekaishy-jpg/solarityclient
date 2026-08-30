//! Layer-count deduplication and renderer-lifetime pipeline ownership.

#![allow(unsafe_code)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use ash::{Device, vk};

use crate::device::VulkanError;
use crate::{TerrainLayerCount, TerrainSpirvCompiler};

use super::pipeline::{TerrainPipelineLayout, create_pipeline};
use super::{TerrainPipelineHandle, TerrainPipelineInfo};

struct GpuTerrainPipeline {
    handle: vk::Pipeline,
    info: TerrainPipelineInfo,
}

pub(in crate::device) struct TerrainPipelineRegistry {
    registry_id: u64,
    compiler: Option<TerrainSpirvCompiler>,
    layout: TerrainPipelineLayout,
    handles: HashMap<TerrainLayerCount, TerrainPipelineHandle>,
    resources: Vec<GpuTerrainPipeline>,
}

impl Default for TerrainPipelineRegistry {
    fn default() -> Self {
        static NEXT_REGISTRY_ID: AtomicU64 = AtomicU64::new(1);
        Self {
            registry_id: NEXT_REGISTRY_ID.fetch_add(1, Ordering::Relaxed),
            compiler: None,
            layout: TerrainPipelineLayout::default(),
            handles: HashMap::new(),
            resources: Vec::new(),
        }
    }
}

impl TerrainPipelineRegistry {
    pub(in crate::device) fn material_set_layout(
        &mut self,
        device: &Device,
    ) -> Result<vk::DescriptorSetLayout, VulkanError> {
        self.layout.ensure_created(device)?;
        self.layout.descriptor_set(1).ok_or_else(|| {
            VulkanError::operation(
                "access terrain material descriptor layout",
                "layout is unavailable",
            )
        })
    }

    pub(in crate::device) fn prepare(
        &mut self,
        device: &Device,
        color_format: vk::Format,
        depth_format: vk::Format,
        layer_count: TerrainLayerCount,
    ) -> Result<TerrainPipelineHandle, VulkanError> {
        if let Some(handle) = self.handles.get(&layer_count) {
            return Ok(*handle);
        }
        let slot = u32::try_from(self.resources.len())
            .map_err(|_source| VulkanError::TerrainPipelineCapacity)?;
        self.layout.ensure_created(device)?;
        if self.compiler.is_none() {
            self.compiler = Some(TerrainSpirvCompiler::new().map_err(shader_error)?);
        }
        let compiler = self.compiler.as_ref().ok_or_else(|| {
            VulkanError::operation("access terrain shader compiler", "compiler is unavailable")
        })?;
        let program = compiler.compile(layer_count).map_err(shader_error)?;
        let pipeline = create_pipeline(
            device,
            self.layout.handle(),
            color_format,
            depth_format,
            &program,
        )?;
        let handle = TerrainPipelineHandle {
            registry_id: self.registry_id,
            slot,
        };
        self.resources.push(GpuTerrainPipeline {
            handle: pipeline,
            info: TerrainPipelineInfo::new(layer_count),
        });
        self.handles.insert(layer_count, handle);
        Ok(handle)
    }

    pub(in crate::device) fn info(
        &self,
        handle: TerrainPipelineHandle,
    ) -> Option<TerrainPipelineInfo> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .map(|resource| resource.info)
    }

    pub(in crate::device) fn destroy(&mut self, device: &Device) {
        self.handles.clear();
        // SAFETY: Renderer idle guarantees no pipeline is in flight.
        unsafe {
            for resource in self.resources.drain(..).rev() {
                device.destroy_pipeline(resource.handle, None);
            }
        }
        self.layout.destroy(device);
    }
}

fn shader_error(error: impl ToString) -> VulkanError {
    VulkanError::TerrainShader {
        message: error.to_string(),
    }
}
