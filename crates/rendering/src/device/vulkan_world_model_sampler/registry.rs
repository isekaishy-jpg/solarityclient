//! Four-axis variants across stock filtering and base-mip settings.

#![allow(unsafe_code)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use ash::{Device, vk};

use crate::WorldModelMaterialState;
use crate::device::VulkanError;

use super::{
    WorldModelBaseMip, WorldModelSamplerHandle, WorldModelSamplerInfo,
    WorldModelTextureAddressMode, WorldModelTextureFiltering,
};

struct GpuWorldModelSampler {
    handle: vk::Sampler,
    info: WorldModelSamplerInfo,
}

pub(in crate::device) struct WorldModelSamplerRegistry {
    registry_id: u64,
    handles: HashMap<WorldModelSamplerInfo, WorldModelSamplerHandle>,
    resources: Vec<GpuWorldModelSampler>,
}

impl Default for WorldModelSamplerRegistry {
    fn default() -> Self {
        static NEXT_REGISTRY_ID: AtomicU64 = AtomicU64::new(1);

        Self {
            registry_id: NEXT_REGISTRY_ID.fetch_add(1, Ordering::Relaxed),
            handles: HashMap::new(),
            resources: Vec::new(),
        }
    }
}

impl WorldModelSamplerRegistry {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::device) fn prepare(
        &mut self,
        device: &Device,
        material: WorldModelMaterialState,
        filtering: WorldModelTextureFiltering,
        base_mip: WorldModelBaseMip,
        anisotropy_supported: bool,
        maximum_anisotropy: f32,
    ) -> Result<WorldModelSamplerHandle, VulkanError> {
        let clamps = material.texture_clamps();
        let address_u = address_mode(clamps[0]);
        let address_v = address_mode(clamps[1]);
        let requested = filtering.requested_anisotropy();
        let effective = if anisotropy_supported && requested > 1.0 {
            requested.min(maximum_anisotropy.max(1.0))
        } else {
            1.0
        };
        let info = WorldModelSamplerInfo::new(filtering, base_mip, address_u, address_v, effective);
        if let Some(handle) = self.handles.get(&info) {
            return Ok(*handle);
        }
        let slot = u32::try_from(self.resources.len())
            .map_err(|_source| VulkanError::WorldModelSamplerCapacity)?;
        let create_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(if filtering.uses_linear_mips() {
                vk::SamplerMipmapMode::LINEAR
            } else {
                vk::SamplerMipmapMode::NEAREST
            })
            .address_mode_u(vulkan_address(address_u))
            .address_mode_v(vulkan_address(address_v))
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .mip_lod_bias(0.0)
            .anisotropy_enable(effective > 1.0)
            .max_anisotropy(effective)
            .compare_enable(false)
            .compare_op(vk::CompareOp::NEVER)
            .min_lod(base_mip.level())
            .max_lod(vk::LOD_CLAMP_NONE)
            .border_color(vk::BorderColor::FLOAT_TRANSPARENT_BLACK)
            .unnormalized_coordinates(false);
        // SAFETY: Requested anisotropy was capped to an enabled device feature
        // and limit; all remaining fields are self-contained core state.
        let sampler = unsafe { device.create_sampler(&create_info, None) }
            .map_err(|source| VulkanError::operation("create WMO texture sampler", source))?;
        let handle = WorldModelSamplerHandle {
            registry_id: self.registry_id,
            slot,
        };
        self.resources.push(GpuWorldModelSampler {
            handle: sampler,
            info,
        });
        self.handles.insert(info, handle);
        Ok(handle)
    }

    pub(in crate::device) fn info(
        &self,
        handle: WorldModelSamplerHandle,
    ) -> Option<WorldModelSamplerInfo> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .map(|resource| resource.info)
    }

    pub(in crate::device) fn handle(&self, handle: WorldModelSamplerHandle) -> Option<vk::Sampler> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .map(|resource| resource.handle)
    }

    pub(in crate::device) fn destroy(&mut self, device: &Device) {
        self.handles.clear();
        // SAFETY: Renderer teardown retires every submitted descriptor use.
        unsafe {
            for resource in self.resources.drain(..).rev() {
                device.destroy_sampler(resource.handle, None);
            }
        }
    }
}

const fn address_mode(clamp: bool) -> WorldModelTextureAddressMode {
    if clamp {
        WorldModelTextureAddressMode::Clamp
    } else {
        WorldModelTextureAddressMode::Repeat
    }
}

const fn vulkan_address(mode: WorldModelTextureAddressMode) -> vk::SamplerAddressMode {
    match mode {
        WorldModelTextureAddressMode::Clamp => vk::SamplerAddressMode::CLAMP_TO_EDGE,
        WorldModelTextureAddressMode::Repeat => vk::SamplerAddressMode::REPEAT,
    }
}
