//! Cached stock-size model portraits rendered directly into sampled GPU images.

#![allow(unsafe_code)]

mod image;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use ash::{Device, vk};
use glam::Mat4;

use crate::device::VulkanError;
use crate::device::vulkan_m2_draw::M2PreparedDraw;
use crate::device::vulkan_m2_pipeline::M2PipelineRegistry;
use crate::device::vulkan_m2_texture_set::M2TextureSetRegistry;
use crate::device::vulkan_mesh::M2MeshRegistry;
use crate::device::vulkan_ui_draw::UiPreparedDraw;
use crate::device::vulkan_ui_frame::UiOverlayRecordContext;
use crate::device::vulkan_ui_mesh::UiMeshRegistry;
use crate::device::vulkan_ui_pipeline::UiPipelineRegistry;
use crate::device::vulkan_ui_texture_set::UiTextureSetRegistry;
use crate::model::M2SceneUniform;

use super::command::{RecordContext, record_draws};
use super::resource::{FrameCreateContext, M2FrameResources};
use image::PortraitImage;

const EXTENT: (u32, u32) = (64, 64);

/// Stable renderer-local image identity; appearance updates preserve its descriptors.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct UiPortraitTextureHandle {
    registry_id: u64,
    slot: u32,
}

pub(in crate::device) struct PortraitRenderContext<'a> {
    pub device: &'a Device,
    pub allocator: &'a vk_mem::Allocator,
    pub graphics_queue: vk::Queue,
    pub graphics_queue_family: u32,
    pub color_format: vk::Format,
    pub depth_format: vk::Format,
    pub uniform_alignment: vk::DeviceSize,
    pub storage_alignment: vk::DeviceSize,
    pub frame_layouts: [vk::DescriptorSetLayout; 3],
    pub pipelines: &'a M2PipelineRegistry,
    pub meshes: &'a M2MeshRegistry,
    pub texture_sets: &'a M2TextureSetRegistry,
    pub ui_pipelines: &'a UiPipelineRegistry,
    pub ui_meshes: &'a UiMeshRegistry,
    pub ui_texture_sets: &'a UiTextureSetRegistry,
    pub mask: UiPreparedDraw,
}

struct PortraitEntry {
    unit: String,
    image: PortraitImage,
    ready: bool,
}

pub(in crate::device) struct PortraitRegistry {
    registry_id: u64,
    handles: HashMap<String, UiPortraitTextureHandle>,
    resources: Vec<PortraitEntry>,
    frames: M2FrameResources,
}

impl Default for PortraitRegistry {
    fn default() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        Self {
            registry_id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            handles: HashMap::new(),
            resources: Vec::new(),
            frames: M2FrameResources::default(),
        }
    }
}

impl PortraitRegistry {
    pub(in crate::device) fn render(
        &mut self,
        context: PortraitRenderContext<'_>,
        unit: &str,
        scene: M2SceneUniform,
        bones: &[Mat4],
        draws: &[M2PreparedDraw],
    ) -> Result<UiPortraitTextureHandle, VulkanError> {
        if draws.is_empty() {
            return Err(VulkanError::EmptyM2Frame);
        }
        for draw in draws {
            let pipeline = context
                .pipelines
                .info(draw.pipeline())
                .ok_or(VulkanError::UnknownM2PipelineHandle)?;
            if pipeline.permutation().has_shadows() {
                return Err(VulkanError::M2ShadowResourcesUnavailable);
            }
            if draw.required_bone_transforms() > bones.len() {
                return Err(VulkanError::M2FrameBoneTransforms {
                    required: draw.required_bone_transforms(),
                    available: bones.len(),
                });
            }
        }
        self.frames.ensure(FrameCreateContext {
            device: context.device,
            allocator: context.allocator,
            descriptor_layouts: context.frame_layouts,
            graphics_queue_family: context.graphics_queue_family,
            slot_count: 1,
            draw_capacity: draws.len(),
            bone_capacity: bones.len(),
            uniform_alignment: context.uniform_alignment,
            storage_alignment: context.storage_alignment,
            extent: EXTENT,
            depth_format: context.depth_format,
        })?;
        let handle = if let Some(handle) = self.handles.get(unit) {
            *handle
        } else {
            let slot = u32::try_from(self.resources.len())
                .map_err(|source| VulkanError::operation("allocate portrait identity", source))?;
            let image = PortraitImage::create(
                context.device,
                context.allocator,
                context.color_format,
                EXTENT,
            )?;
            let handle = UiPortraitTextureHandle {
                registry_id: self.registry_id,
                slot,
            };
            self.resources.push(PortraitEntry {
                unit: unit.to_owned(),
                image,
                ready: false,
            });
            self.handles.insert(unit.to_owned(), handle);
            handle
        };
        let entry = &mut self.resources[handle.slot as usize];
        let slot = self.frames.slot_mut(0)?;
        slot.wait_and_reset(context.device)?;
        slot.write(context.allocator, scene, bones, draws)?;
        let mask_draws = [context.mask];
        record_draws(RecordContext {
            device: context.device,
            capture: None,
            command_buffer: slot.command_buffer(),
            image: entry.image.image,
            image_view: entry.image.view,
            depth_image: slot.depth_image(),
            depth_view: slot.depth_view(),
            extent: EXTENT,
            frame_sets: slot.descriptor_sets(),
            material_stride: slot.material_stride(),
            pipelines: context.pipelines,
            meshes: context.meshes,
            texture_sets: context.texture_sets,
            draws,
            sampled_output: true,
            mask: Some(UiOverlayRecordContext {
                device: context.device,
                command_buffer: slot.command_buffer(),
                image_view: entry.image.view,
                extent: EXTENT,
                logical_extent: [64.0, 64.0],
                pipelines: context.ui_pipelines,
                meshes: context.ui_meshes,
                texture_sets: context.ui_texture_sets,
                draws: &mask_draws,
                overlay: &[],
            }),
        })?;
        let commands =
            [vk::CommandBufferSubmitInfo::default().command_buffer(slot.command_buffer())];
        let submits = [vk::SubmitInfo2::default().command_buffer_infos(&commands)];
        slot.reset_fence(context.device)?;
        // SAFETY: One graphics queue orders earlier UI reads before the portrait's
        // fragment-to-color execution barrier, and this fence protects slot reuse.
        if let Err(source) = unsafe {
            context
                .device
                .queue_submit2(context.graphics_queue, &submits, slot.fence())
        } {
            slot.restore_signaled_fence(context.device)?;
            return Err(VulkanError::operation("submit portrait render", source));
        }
        entry.ready = true;
        Ok(handle)
    }

    pub(in crate::device) fn handle(&self, unit: &str) -> Option<UiPortraitTextureHandle> {
        let handle = *self.handles.get(unit)?;
        self.entry(handle).map(|_| handle)
    }

    fn entry(&self, handle: UiPortraitTextureHandle) -> Option<&PortraitEntry> {
        (handle.registry_id == self.registry_id).then_some(())?;
        self.resources
            .get(handle.slot as usize)
            .filter(|entry| entry.ready)
    }

    pub(in crate::device) fn unit(&self, handle: UiPortraitTextureHandle) -> Option<&str> {
        self.entry(handle).map(|entry| entry.unit.as_str())
    }

    pub(in crate::device) fn view(&self, handle: UiPortraitTextureHandle) -> Option<vk::ImageView> {
        self.entry(handle).map(|entry| entry.image.view)
    }

    pub(in crate::device) fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        self.frames.destroy(device, allocator);
        self.handles.clear();
        for mut entry in self.resources.drain(..).rev() {
            entry.image.destroy(device, allocator);
        }
    }
}
