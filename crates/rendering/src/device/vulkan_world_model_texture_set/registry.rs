//! Exact-size descriptor allocation and renderer-lifetime ownership.

#![allow(unsafe_code)]

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use ash::{Device, vk};

use crate::device::VulkanError;
use crate::device::vulkan_texture::BlpTextureRegistry;
use crate::device::vulkan_world_model_sampler::WorldModelSamplerRegistry;

use super::{WorldModelTextureSet, WorldModelTextureSetHandle, WorldModelTextureSetInfo};

struct GpuWorldModelTextureSet {
    handle: vk::DescriptorSet,
    info: WorldModelTextureSetInfo,
    request: WorldModelTextureSet,
}

pub(in crate::device) struct WorldModelTextureSetRegistry {
    registry_id: u64,
    handles: HashMap<WorldModelTextureSet, WorldModelTextureSetHandle>,
    resources: Vec<GpuWorldModelTextureSet>,
    pools: Vec<vk::DescriptorPool>,
}

impl Default for WorldModelTextureSetRegistry {
    fn default() -> Self {
        static NEXT_REGISTRY_ID: AtomicU64 = AtomicU64::new(1);

        Self {
            registry_id: NEXT_REGISTRY_ID.fetch_add(1, Ordering::Relaxed),
            handles: HashMap::new(),
            resources: Vec::new(),
            pools: Vec::new(),
        }
    }
}

impl WorldModelTextureSetRegistry {
    pub(in crate::device) fn prepare(
        &mut self,
        device: &Device,
        layout: vk::DescriptorSetLayout,
        textures: &BlpTextureRegistry,
        samplers: &WorldModelSamplerRegistry,
        requested: &[WorldModelTextureSet],
    ) -> Result<Vec<WorldModelTextureSetHandle>, VulkanError> {
        validate_resources(textures, samplers, requested)?;
        let mut seen = HashSet::new();
        let pending = requested
            .iter()
            .copied()
            .filter(|set| !self.handles.contains_key(set) && seen.insert(*set))
            .collect::<Vec<_>>();
        if !pending.is_empty() {
            self.allocate_batch(device, layout, textures, samplers, &pending)?;
        }
        requested
            .iter()
            .map(|set| {
                self.handles.get(set).copied().ok_or_else(|| {
                    VulkanError::operation(
                        "resolve WMO texture descriptor set",
                        "prepared set is unavailable",
                    )
                })
            })
            .collect()
    }

    pub(in crate::device) fn info(
        &self,
        handle: WorldModelTextureSetHandle,
    ) -> Option<WorldModelTextureSetInfo> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .map(|resource| resource.info)
    }

    pub(in crate::device) fn raw(
        &self,
        handle: WorldModelTextureSetHandle,
    ) -> Option<vk::DescriptorSet> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .map(|resource| resource.handle)
    }

    pub(in crate::device) fn request(
        &self,
        handle: WorldModelTextureSetHandle,
    ) -> Option<WorldModelTextureSet> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .map(|resource| resource.request)
    }

    pub(in crate::device) fn destroy(&mut self, device: &Device) {
        self.handles.clear();
        self.resources.clear();
        // SAFETY: Renderer teardown retires all descriptor use first.
        unsafe {
            for pool in self.pools.drain(..).rev() {
                device.destroy_descriptor_pool(pool, None);
            }
        }
    }

    fn allocate_batch(
        &mut self,
        device: &Device,
        layout: vk::DescriptorSetLayout,
        textures: &BlpTextureRegistry,
        samplers: &WorldModelSamplerRegistry,
        pending: &[WorldModelTextureSet],
    ) -> Result<(), VulkanError> {
        let first_slot = u32::try_from(self.resources.len())
            .map_err(|_source| VulkanError::WorldModelTextureSetCapacity)?;
        let added = u32::try_from(pending.len())
            .map_err(|_source| VulkanError::WorldModelTextureSetCapacity)?;
        first_slot
            .checked_add(added)
            .ok_or(VulkanError::WorldModelTextureSetCapacity)?;
        let descriptor_count = added
            .checked_mul(2)
            .ok_or(VulkanError::WorldModelTextureSetCapacity)?;
        let pool_sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(descriptor_count)];
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(added)
            .pool_sizes(&pool_sizes);
        // SAFETY: Pending is nonempty and all counts were checked.
        let pool =
            unsafe { device.create_descriptor_pool(&pool_info, None) }.map_err(|source| {
                VulkanError::operation("create WMO texture descriptor pool", source)
            })?;
        let layouts = vec![layout; pending.len()];
        let allocate_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(pool)
            .set_layouts(&layouts);
        // SAFETY: Pool and compatible repeated layouts remain live.
        let sets = match unsafe { device.allocate_descriptor_sets(&allocate_info) } {
            Ok(sets) => sets,
            Err(source) => {
                // SAFETY: No successful allocation escaped the failed pool.
                unsafe { device.destroy_descriptor_pool(pool, None) };
                return Err(VulkanError::operation(
                    "allocate WMO texture descriptor sets",
                    source,
                ));
            }
        };
        for (key, descriptor_set) in pending.iter().copied().zip(sets) {
            write_texture_set(device, descriptor_set, textures, samplers, key)?;
            let slot = u32::try_from(self.resources.len())
                .map_err(|_source| VulkanError::WorldModelTextureSetCapacity)?;
            let handle = WorldModelTextureSetHandle {
                registry_id: self.registry_id,
                slot,
            };
            self.resources.push(GpuWorldModelTextureSet {
                handle: descriptor_set,
                info: WorldModelTextureSetInfo::new(key.stage_count()),
                request: key,
            });
            self.handles.insert(key, handle);
        }
        self.pools.push(pool);
        Ok(())
    }
}

fn validate_resources(
    textures: &BlpTextureRegistry,
    samplers: &WorldModelSamplerRegistry,
    sets: &[WorldModelTextureSet],
) -> Result<(), VulkanError> {
    for stage in sets.iter().flat_map(WorldModelTextureSet::stages) {
        if textures.view(stage.texture()).is_none() {
            return Err(VulkanError::UnknownWorldModelTextureHandle);
        }
        if samplers.handle(stage.sampler()).is_none() {
            return Err(VulkanError::UnknownWorldModelSamplerHandle);
        }
    }
    Ok(())
}

fn write_texture_set(
    device: &Device,
    descriptor_set: vk::DescriptorSet,
    textures: &BlpTextureRegistry,
    samplers: &WorldModelSamplerRegistry,
    set: WorldModelTextureSet,
) -> Result<(), VulkanError> {
    for (binding, stage) in set.stages().iter().copied().enumerate() {
        let image_view = textures
            .view(stage.texture())
            .ok_or(VulkanError::UnknownWorldModelTextureHandle)?;
        let sampler = samplers
            .handle(stage.sampler())
            .ok_or(VulkanError::UnknownWorldModelSamplerHandle)?;
        let image_infos = [vk::DescriptorImageInfo::default()
            .sampler(sampler)
            .image_view(image_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let write =
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(u32::try_from(binding).map_err(|source| {
                    VulkanError::operation("convert WMO texture binding", source)
                })?)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&image_infos);
        // SAFETY: Set, image view, sampler, and declared binding are live.
        unsafe { device.update_descriptor_sets(&[write], &[]) };
    }
    Ok(())
}
