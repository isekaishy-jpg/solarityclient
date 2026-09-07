//! Renderer-lifetime ownership for the three closed liquid shader families.

#![allow(unsafe_code)]

use ash::{Device, vk};

use crate::device::VulkanError;
use crate::{LiquidShader, LiquidSpirvProgram};

use super::pipeline::{LiquidPipelineLayout, create_pipeline};

/// Each family has one pipeline on the programmable user-clipping path.
#[derive(Default)]
pub(in crate::device) struct LiquidPipelines {
    layout: LiquidPipelineLayout,
    pipelines: [vk::Pipeline; 3],
}

impl LiquidPipelines {
    /// Creates the complete family before any frame references its shared ABI.
    pub(in crate::device) fn prepare(
        &mut self,
        device: &Device,
        color_format: vk::Format,
        depth_format: vk::Format,
    ) -> Result<(), VulkanError> {
        self.layout.ensure_created(device)?;
        for shader in [
            LiquidShader::Water,
            LiquidShader::WaterNoSpecular,
            LiquidShader::Magma,
        ] {
            let slot = index(shader);
            if self.pipelines[slot] == vk::Pipeline::null() {
                self.pipelines[slot] = create_pipeline(
                    device,
                    self.layout.handle(),
                    color_format,
                    depth_format,
                    &LiquidSpirvProgram::for_shader(shader),
                )?;
            }
        }
        Ok(())
    }

    /// Returns one initialized family and the shared descriptor layout.
    pub(in crate::device) fn raw(
        &self,
        shader: LiquidShader,
    ) -> (vk::Pipeline, vk::PipelineLayout) {
        (self.pipelines[index(shader)], self.layout.handle())
    }

    /// Returns the draw and material layouts after preparation has succeeded.
    pub(in crate::device) fn descriptor_layouts(
        &self,
    ) -> Result<[vk::DescriptorSetLayout; 2], VulkanError> {
        let layout = |index| {
            self.layout.descriptor_set(index).ok_or_else(|| {
                VulkanError::operation("access liquid descriptor layout", "layout is unavailable")
            })
        };
        Ok([layout(0)?, layout(1)?])
    }

    /// Releases family pipelines before their shared layouts at renderer teardown.
    pub(in crate::device) fn destroy(&mut self, device: &Device) {
        for pipeline in &mut self.pipelines {
            if *pipeline != vk::Pipeline::null() {
                // SAFETY: Renderer idle proves every use of these owned pipelines retired.
                unsafe { device.destroy_pipeline(*pipeline, None) };
                *pipeline = vk::Pipeline::null();
            }
        }
        self.layout.destroy(device);
    }
}

/// Keeps shader identity independent of Rust enum discriminant layout.
const fn index(shader: LiquidShader) -> usize {
    match shader {
        LiquidShader::Water => 0,
        LiquidShader::WaterNoSpecular => 1,
        LiquidShader::Magma => 2,
    }
}
