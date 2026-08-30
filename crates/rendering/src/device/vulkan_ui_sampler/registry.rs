//! Four-state UI sampler deduplication and Vulkan lifetime ownership.

#![allow(unsafe_code)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use ash::{Device, vk};

use crate::UiTextureAddressMode;
use crate::device::VulkanError;

use super::{UiSamplerHandle, UiSamplerInfo};

/// One driver sampler and its stock-facing state.
struct GpuUiSampler {
    handle: vk::Sampler,
    info: UiSamplerInfo,
}

/// Owns the closed set of horizontal/vertical tiling combinations.
pub(in crate::device) struct UiSamplerRegistry {
    registry_id: u64,
    handles: HashMap<UiSamplerInfo, UiSamplerHandle>,
    resources: Vec<GpuUiSampler>,
}

impl Default for UiSamplerRegistry {
    /// Assigns process-unique locality and reserves the exact four-state domain.
    fn default() -> Self {
        static NEXT_REGISTRY_ID: AtomicU64 = AtomicU64::new(1);

        Self {
            registry_id: NEXT_REGISTRY_ID.fetch_add(1, Ordering::Relaxed),
            handles: HashMap::with_capacity(4),
            resources: Vec::with_capacity(4),
        }
    }
}

impl UiSamplerRegistry {
    /// Returns an existing sampler or creates one exact tiling combination.
    pub(in crate::device) fn prepare(
        &mut self,
        device: &Device,
        info: UiSamplerInfo,
    ) -> Result<UiSamplerHandle, VulkanError> {
        if let Some(handle) = self.handles.get(&info) {
            return Ok(*handle);
        }
        let slot = u32::try_from(self.resources.len())
            .map_err(|_source| VulkanError::UiSamplerCapacity)?;
        let create_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .address_mode_u(vulkan_address(info.horizontal()))
            .address_mode_v(vulkan_address(info.vertical()))
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .mip_lod_bias(0.0)
            .anisotropy_enable(false)
            .max_anisotropy(1.0)
            .compare_enable(false)
            .compare_op(vk::CompareOp::NEVER)
            .min_lod(0.0)
            .max_lod(vk::LOD_CLAMP_NONE)
            .border_color(vk::BorderColor::FLOAT_TRANSPARENT_BLACK)
            .unnormalized_coordinates(false);
        // SAFETY: The create info requests only core sampler state from this device.
        let sampler = unsafe { device.create_sampler(&create_info, None) }
            .map_err(|source| VulkanError::operation("create UI texture sampler", source))?;
        let handle = UiSamplerHandle {
            registry_id: self.registry_id,
            slot,
        };
        self.resources.push(GpuUiSampler {
            handle: sampler,
            info,
        });
        self.handles.insert(info, handle);
        Ok(handle)
    }

    /// Returns immutable state for one renderer-local sampler.
    pub(in crate::device) fn info(&self, handle: UiSamplerHandle) -> Option<UiSamplerInfo> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .map(|resource| resource.info)
    }

    /// Releases every sampler after submitted draws are idle.
    pub(in crate::device) fn destroy(&mut self, device: &Device) {
        self.handles.clear();
        // SAFETY: Every sampler belongs to this device and is uniquely owned.
        unsafe {
            for resource in self.resources.drain(..).rev() {
                device.destroy_sampler(resource.handle, None);
            }
        }
    }
}

/// Converts the closed UI address domain without a default branch.
const fn vulkan_address(mode: UiTextureAddressMode) -> vk::SamplerAddressMode {
    match mode {
        UiTextureAddressMode::Clamp => vk::SamplerAddressMode::CLAMP_TO_EDGE,
        UiTextureAddressMode::Repeat => vk::SamplerAddressMode::REPEAT,
    }
}
