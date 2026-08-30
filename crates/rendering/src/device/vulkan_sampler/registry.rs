//! M2 texture-flag deduplication and Vulkan sampler lifetime ownership.

#![allow(unsafe_code)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use ash::{Device, vk};
use solarity_asset::M2Texture;

use crate::device::VulkanError;

use super::types::{M2SamplerHandle, M2SamplerInfo, M2TextureAddressMode};

const WRAP_U_FLAG: u32 = 0x1;
const WRAP_V_FLAG: u32 = 0x2;

/// One driver sampler and its stock-facing addressing state.
struct GpuM2Sampler {
    handle: vk::Sampler,
    info: M2SamplerInfo,
}

/// Owns the small set of sampler combinations used by M2 texture declarations.
pub(in crate::device) struct M2SamplerRegistry {
    registry_id: u64,
    handles: HashMap<M2SamplerInfo, M2SamplerHandle>,
    resources: Vec<GpuM2Sampler>,
}

impl Default for M2SamplerRegistry {
    /// Assigns process-unique renderer locality without creating a sampler.
    fn default() -> Self {
        static NEXT_REGISTRY_ID: AtomicU64 = AtomicU64::new(1);

        Self {
            registry_id: NEXT_REGISTRY_ID.fetch_add(1, Ordering::Relaxed),
            handles: HashMap::new(),
            resources: Vec::new(),
        }
    }
}

impl M2SamplerRegistry {
    /// Returns an existing sampler or creates the exact authored wrap state.
    pub(in crate::device) fn prepare(
        &mut self,
        device: &Device,
        texture: &M2Texture,
    ) -> Result<M2SamplerHandle, VulkanError> {
        let info = sampler_info(texture.flags());
        if let Some(handle) = self.handles.get(&info) {
            return Ok(*handle);
        }
        let slot = u32::try_from(self.resources.len())
            .map_err(|_source| VulkanError::M2SamplerCapacity)?;
        let create_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            // Stock GxTex_Linear disables mip filtering. Constraining both LOD
            // bounds to zero makes the Vulkan mipmap mode unobservable.
            .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
            .address_mode_u(vulkan_address(info.address_u()))
            .address_mode_v(vulkan_address(info.address_v()))
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .mip_lod_bias(0.0)
            .anisotropy_enable(false)
            .max_anisotropy(1.0)
            .compare_enable(false)
            .compare_op(vk::CompareOp::NEVER)
            .min_lod(0.0)
            .max_lod(0.0)
            .border_color(vk::BorderColor::FLOAT_TRANSPARENT_BLACK)
            .unnormalized_coordinates(false);
        // SAFETY: The create info is self-contained and requests only core
        // sampler state from this live device.
        let sampler = unsafe { device.create_sampler(&create_info, None) }
            .map_err(|source| VulkanError::operation("create M2 texture sampler", source))?;
        let handle = M2SamplerHandle {
            registry_id: self.registry_id,
            slot,
        };
        self.resources.push(GpuM2Sampler {
            handle: sampler,
            info,
        });
        self.handles.insert(info, handle);
        Ok(handle)
    }

    /// Resolves renderer-local diagnostics without exposing Vulkan handles.
    pub(in crate::device) fn info(&self, handle: M2SamplerHandle) -> Option<M2SamplerInfo> {
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
        // SAFETY: Every sampler belongs to this device, is no longer in flight,
        // and is uniquely owned by the registry.
        unsafe {
            for resource in self.resources.drain(..).rev() {
                device.destroy_sampler(resource.handle, None);
            }
        }
    }
}

/// Translates only the two M2 flags consulted by stock texture setup.
const fn sampler_info(flags: u32) -> M2SamplerInfo {
    M2SamplerInfo::new(
        if flags & WRAP_U_FLAG != 0 {
            M2TextureAddressMode::Repeat
        } else {
            M2TextureAddressMode::Clamp
        },
        if flags & WRAP_V_FLAG != 0 {
            M2TextureAddressMode::Repeat
        } else {
            M2TextureAddressMode::Clamp
        },
    )
}

/// Converts the closed stock address-mode domain to Vulkan.
const fn vulkan_address(mode: M2TextureAddressMode) -> vk::SamplerAddressMode {
    match mode {
        M2TextureAddressMode::Clamp => vk::SamplerAddressMode::CLAMP_TO_EDGE,
        M2TextureAddressMode::Repeat => vk::SamplerAddressMode::REPEAT,
    }
}
