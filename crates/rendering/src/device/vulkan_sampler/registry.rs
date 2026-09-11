//! M2 texture-flag deduplication and Vulkan sampler lifetime ownership.

#![allow(unsafe_code)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use ash::{Device, vk};
use solarity_asset::M2Texture;

use crate::device::{VulkanError, WorldModelBaseMip, WorldModelTextureFiltering};

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
        self.prepare_info(device, info)
    }

    pub(in crate::device) fn prepare_file(
        &mut self,
        device: &Device,
        texture: &M2Texture,
        filtering: WorldModelTextureFiltering,
        base_mip: WorldModelBaseMip,
        maximum_anisotropy: f32,
    ) -> Result<M2SamplerHandle, VulkanError> {
        let info = sampler_info(texture.flags()).with_file_filtering(
            filtering,
            base_mip,
            maximum_anisotropy,
        );
        self.prepare_info(device, info)
    }

    fn prepare_info(
        &mut self,
        device: &Device,
        info: M2SamplerInfo,
    ) -> Result<M2SamplerHandle, VulkanError> {
        if let Some(handle) = self.handles.get(&info) {
            return Ok(*handle);
        }
        let slot = u32::try_from(self.resources.len())
            .map_err(|_source| VulkanError::M2SamplerCapacity)?;
        let create_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(
                if info
                    .file_filtering()
                    .is_some_and(WorldModelTextureFiltering::uses_linear_mips)
                {
                    vk::SamplerMipmapMode::LINEAR
                } else {
                    vk::SamplerMipmapMode::NEAREST
                },
            )
            .address_mode_u(vulkan_address(info.address_u()))
            .address_mode_v(vulkan_address(info.address_v()))
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .mip_lod_bias(0.0)
            .anisotropy_enable(info.effective_anisotropy() > 1.0)
            .max_anisotropy(info.effective_anisotropy())
            .compare_enable(false)
            .compare_op(vk::CompareOp::NEVER)
            .min_lod(info.base_mip().level())
            .max_lod(if info.file_filtering().is_some() {
                vk::LOD_CLAMP_NONE
            } else {
                0.0
            })
            .border_color(vk::BorderColor::FLOAT_TRANSPARENT_BLACK)
            .unnormalized_coordinates(false);
        // SAFETY: The create info is self-contained. File anisotropy is capped
        // to the enabled feature/limit; explicit linear samplers remain unmipped.
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

    /// Resolves a renderer-local identity to its live Vulkan sampler handle.
    pub(in crate::device) fn handle(&self, handle: M2SamplerHandle) -> Option<vk::Sampler> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .map(|resource| resource.handle)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_texture_filtering_matches_native_selection() -> Result<(), Box<dyn std::error::Error>> {
        let native = include_str!("../../../tests/fixtures/world_texture_filter_native.txt");
        let mut checked = 0;
        for line in native.lines().filter_map(|line| line.strip_prefix("case ")) {
            let (input, output) = line.split_once(';').ok_or("native delimiter")?;
            let values = input
                .split_whitespace()
                .map(str::parse::<u32>)
                .collect::<Result<Vec<_>, _>>()?;
            // Vulkan's sampled UNORM textures support linear mip interpolation.
            // Explicit M2 requests use GxTex_Linear; other explicit classes in
            // the capture belong to independent texture providers.
            if values[1] == 0 || (values[4] != 0 && values[5] & 7 != 1) {
                continue;
            }
            let expected = output
                .split_whitespace()
                .map(str::parse::<u32>)
                .collect::<Result<Vec<_>, _>>()?;
            let mut info = sampler_info(values[5] >> 3);
            if values[4] == 0 {
                let filtering =
                    WorldModelTextureFiltering::from_cvar(values[0] as i32).ok_or("native mode")?;
                info = info.with_file_filtering(
                    filtering,
                    WorldModelBaseMip::Zero,
                    if values[2] == 0 {
                        1.0
                    } else {
                        values[3] as f32
                    },
                );
            }
            let flags = expected[2];
            assert_eq!(info.file_filtering().is_some(), flags & 7 >= 3, "{input}");
            assert_eq!(
                info.file_filtering()
                    .is_some_and(WorldModelTextureFiltering::uses_linear_mips),
                flags & 7 >= 4,
                "{input}"
            );
            assert_eq!(
                info.effective_anisotropy(),
                ((flags >> 9) & 31) as f32,
                "{input}"
            );
            assert_eq!(
                info.address_u() == M2TextureAddressMode::Repeat,
                flags & 8 != 0,
                "{input}"
            );
            assert_eq!(
                info.address_v() == M2TextureAddressMode::Repeat,
                flags & 16 != 0,
                "{input}"
            );
            checked += 1;
        }
        assert_eq!(checked, 384);
        Ok(())
    }
}
