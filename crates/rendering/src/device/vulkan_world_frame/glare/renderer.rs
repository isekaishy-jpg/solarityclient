//! Deferred disc visibility and post-world additive celestial glare.

use super::slot::GlareSlot;

use crate::device::vulkan_pct_pipeline::{PctPipeline, PctPipelineKind};
use crate::device::vulkan_texture::BlpTextureRegistry;
use crate::weather::glare::GlareState;
use crate::{VulkanError, WorldCelestialDraw, WorldCelestialMesh, WorldGlareFrame, WorldGlareKind};
use ash::{Device, vk};

/// Renderer-lifetime fade state and pipelines; buffers and queries belong to slots.
#[derive(Default)]
pub(in crate::device::vulkan_world_frame) struct GlareRenderer {
    states: [GlareState; 2],
    samples: [u64; 2],
    queried: [bool; 2],
    visible: PctPipeline,
    probe: PctPipeline,
}

impl GlareRenderer {
    /// The next world frame consumes the last completed CPU glare update.
    pub(in crate::device::vulkan_world_frame) fn lighting(&self) -> crate::WorldGlareLighting {
        crate::WorldGlareLighting::new(self.states[0].lighting_response())
    }

    /// The count-based native path needs precise sample counts, not boolean queries.
    pub(in crate::device::vulkan_world_frame) fn ensure(
        &mut self,
        device: &Device,
        color: vk::Format,
        depth: vk::Format,
        precise: bool,
    ) -> Result<(), VulkanError> {
        if !precise {
            return Err(VulkanError::operation(
                "prepare celestial glare",
                "precise occlusion queries are unavailable",
            ));
        }
        self.visible
            .prepare(device, color, depth, PctPipelineKind::Glare)?;
        self.probe
            .prepare(device, color, depth, PctPipelineKind::GlareProbe)
    }

    /// Collects retired counts without waiting and updates both native owners once.
    #[allow(clippy::too_many_arguments)] // Independent Vulkan owners accompany one immutable scene frame.
    pub(in crate::device::vulkan_world_frame) fn prepare(
        &mut self,
        slot: &mut GlareSlot,
        device: &Device,
        allocator: &vk_mem::Allocator,
        textures: &BlpTextureRegistry,
        frame: Option<WorldGlareFrame>,
        viewport_size: [f32; 2],
    ) -> Result<(), VulkanError> {
        for (index, result) in slot.collect(device)?.into_iter().enumerate() {
            if let Some(samples) = result {
                self.samples[index] = samples;
            }
        }
        slot.clear();
        let Some(frame) = frame else {
            return Ok(());
        };
        slot.ensure(device)?;
        for index in 0..2 {
            let body = frame.bodies[index];
            let probe = WorldCelestialMesh::unclipped(body.size(), frame.colors[index]);
            let probe_draw =
                WorldCelestialDraw::new(&probe, body, frame.textures[index], frame.camera);
            let matrix = probe_draw.view_projection();
            let clip = [0, 3].map(|vertex| {
                matrix * glam::Vec3::from_array(probe.positions()[vertex]).extend(1.)
            });
            // 6BFB60 projects the uncut disc; 9ABE00 divides the retained count
            // by its current screen rectangle, requiring an area above one pixel.
            let area = if clip.iter().all(|p| p.w.abs() >= 0.001) {
                let a = clip[0].truncate() / clip[0].w;
                let b = clip[1].truncate() / clip[1].w;
                ((a.x - b.x).abs() * viewport_size[0] * 0.5)
                    * ((a.y - b.y).abs() * viewport_size[1] * 0.5)
            } else {
                0.
            };
            let visibility = if self.queried[index] && area > 1. {
                (self.samples[index] as f64 / f64::from(area)).min(1.) as f32
            } else {
                0.
            };
            let sample = self.states[index].update(
                if index == 0 {
                    WorldGlareKind::Sun
                } else {
                    WorldGlareKind::Moon
                },
                frame.environment,
                body.position() - frame.camera.camera().position(),
                body.size(),
                frame.camera.forward(),
                frame.colors[index],
                visibility,
            );
            tracing::trace!(target: "solarity_rendering::glare", index,
                cloud = frame.environment.cloud_alpha[index],
                water = ?frame.environment.liquid_depth,
                sky = frame.environment.skybox_weight,
                samples = self.samples[index], area, visibility,
                color = sample.color, size = sample.size,
                "prepared celestial glare");
            if sample.query {
                self.queried[index] = true;
                slot.write_probe(
                    index,
                    device,
                    allocator,
                    textures,
                    self.probe.descriptor_layout(),
                    probe_draw,
                )?;
            }
            if sample.color >> 24 != 0 {
                let mesh = WorldCelestialMesh::unclipped(sample.size, sample.color);
                let draw =
                    WorldCelestialDraw::new(&mesh, body, frame.textures[index], frame.camera);
                slot.write_visible(
                    index,
                    device,
                    allocator,
                    textures,
                    self.visible.descriptor_layout(),
                    draw,
                )?;
            }
        }
        Ok(())
    }

    /// Records the native query then additive glare for sun and first moon.
    pub(in crate::device::vulkan_world_frame) fn record(
        &self,
        slot: &GlareSlot,
        device: &Device,
        command: vk::CommandBuffer,
        viewport: vk::Viewport,
    ) {
        slot.record(
            device,
            command,
            viewport,
            self.probe.raw(),
            self.visible.raw(),
        );
    }

    /// Called only after all frame fences have retired their pipeline users.
    pub(in crate::device::vulkan_world_frame) fn destroy(&mut self, device: &Device) {
        self.visible.destroy(device);
        self.probe.destroy(device);
    }
}
