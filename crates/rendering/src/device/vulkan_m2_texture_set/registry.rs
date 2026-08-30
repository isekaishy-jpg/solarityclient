//! Exact-size bulk descriptor allocation and renderer-lifetime ownership.

#![allow(unsafe_code)]

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use ash::{Device, vk};

use crate::device::VulkanError;
use crate::device::vulkan_sampler::M2SamplerRegistry;
use crate::device::vulkan_texture::BlpTextureRegistry;

use super::types::{M2TextureSet, M2TextureSetHandle, M2TextureSetInfo};

/// One live descriptor set owned transitively by a registry descriptor pool.
struct GpuM2TextureSet {
    #[expect(
        dead_code,
        reason = "retained for the immediately following indexed-draw binding boundary"
    )]
    handle: vk::DescriptorSet,
    info: M2TextureSetInfo,
}

/// Caches material texture-stage sets and owns exact-size pool batches.
pub(in crate::device) struct M2TextureSetRegistry {
    registry_id: u64,
    handles: HashMap<M2TextureSet, M2TextureSetHandle>,
    resources: Vec<GpuM2TextureSet>,
    pools: Vec<vk::DescriptorPool>,
}

impl Default for M2TextureSetRegistry {
    /// Assigns process-unique locality without reserving descriptor capacity.
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

impl M2TextureSetRegistry {
    /// Resolves a model batch in input order, allocating only unique new sets.
    pub(in crate::device) fn prepare(
        &mut self,
        device: &Device,
        layout: vk::DescriptorSetLayout,
        textures: &BlpTextureRegistry,
        samplers: &M2SamplerRegistry,
        requested: &[M2TextureSet],
    ) -> Result<Vec<M2TextureSetHandle>, VulkanError> {
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
                        "resolve M2 texture descriptor set",
                        "prepared set is unavailable",
                    )
                })
            })
            .collect()
    }

    /// Returns stable diagnostics for a renderer-local descriptor identity.
    pub(in crate::device) fn info(&self, handle: M2TextureSetHandle) -> Option<M2TextureSetInfo> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(handle.slot as usize)
            .map(|resource| resource.info)
    }

    /// Releases descriptor sets transitively by destroying their owning pools.
    pub(in crate::device) fn destroy(&mut self, device: &Device) {
        self.handles.clear();
        self.resources.clear();
        // SAFETY: Every pool belongs to this device and all descriptor use is
        // retired before renderer teardown.
        unsafe {
            for pool in self.pools.drain(..).rev() {
                device.destroy_descriptor_pool(pool, None);
            }
        }
    }

    /// Allocates one pool sized from this exact unique material batch.
    fn allocate_batch(
        &mut self,
        device: &Device,
        layout: vk::DescriptorSetLayout,
        textures: &BlpTextureRegistry,
        samplers: &M2SamplerRegistry,
        pending: &[M2TextureSet],
    ) -> Result<(), VulkanError> {
        let first_slot = u32::try_from(self.resources.len())
            .map_err(|_source| VulkanError::M2TextureSetCapacity)?;
        let added =
            u32::try_from(pending.len()).map_err(|_source| VulkanError::M2TextureSetCapacity)?;
        first_slot
            .checked_add(added)
            .ok_or(VulkanError::M2TextureSetCapacity)?;
        // Vulkan charges pool capacity from the compatible layout rather than
        // from the subset of bindings statically consumed by a shader. The
        // common M2 set-three layout declares two descriptors for every set.
        let descriptor_count = added
            .checked_mul(2)
            .ok_or(VulkanError::M2TextureSetCapacity)?;
        let pool_size = vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(descriptor_count);
        let pool_sizes = [pool_size];
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(added)
            .pool_sizes(&pool_sizes);
        // SAFETY: Counts are nonzero because this method receives a nonempty
        // pending batch and each type contains one or two stages.
        let pool =
            unsafe { device.create_descriptor_pool(&pool_info, None) }.map_err(|source| {
                VulkanError::operation("create M2 texture descriptor pool", source)
            })?;
        let layouts = vec![layout; pending.len()];
        let allocate_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(pool)
            .set_layouts(&layouts);
        // SAFETY: The pool and repeated compatible layouts are live for the call.
        let sets = match unsafe { device.allocate_descriptor_sets(&allocate_info) } {
            Ok(sets) => sets,
            Err(source) => {
                // SAFETY: No successful allocation escaped this failed pool.
                unsafe { device.destroy_descriptor_pool(pool, None) };
                return Err(VulkanError::operation(
                    "allocate M2 texture descriptor sets",
                    source,
                ));
            }
        };

        for (key, descriptor_set) in pending.iter().copied().zip(sets) {
            write_texture_set(device, descriptor_set, textures, samplers, key)?;
            let slot = u32::try_from(self.resources.len())
                .map_err(|_source| VulkanError::M2TextureSetCapacity)?;
            let handle = M2TextureSetHandle {
                registry_id: self.registry_id,
                slot,
            };
            self.resources.push(GpuM2TextureSet {
                handle: descriptor_set,
                info: M2TextureSetInfo::new(key.stage_count()),
            });
            self.handles.insert(key, handle);
        }
        self.pools.push(pool);
        Ok(())
    }
}

/// Rejects handles belonging to another renderer before allocating a pool.
fn validate_resources(
    textures: &BlpTextureRegistry,
    samplers: &M2SamplerRegistry,
    sets: &[M2TextureSet],
) -> Result<(), VulkanError> {
    for stage in sets.iter().flat_map(M2TextureSet::stages) {
        if textures.view(stage.texture()).is_none() {
            return Err(VulkanError::UnknownBlpTextureHandle);
        }
        if samplers.handle(stage.sampler()).is_none() {
            return Err(VulkanError::UnknownM2SamplerHandle);
        }
    }
    Ok(())
}

/// Writes each statically consumed stage to its matching shader binding.
fn write_texture_set(
    device: &Device,
    descriptor_set: vk::DescriptorSet,
    textures: &BlpTextureRegistry,
    samplers: &M2SamplerRegistry,
    set: M2TextureSet,
) -> Result<(), VulkanError> {
    for (binding, stage) in set.stages().iter().copied().enumerate() {
        let image_view = textures
            .view(stage.texture())
            .ok_or(VulkanError::UnknownBlpTextureHandle)?;
        let sampler = samplers
            .handle(stage.sampler())
            .ok_or(VulkanError::UnknownM2SamplerHandle)?;
        let image_info = vk::DescriptorImageInfo::default()
            .sampler(sampler)
            .image_view(image_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        let image_infos = [image_info];
        let write =
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(u32::try_from(binding).map_err(|source| {
                    VulkanError::operation("convert M2 texture binding", source)
                })?)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&image_infos);
        // SAFETY: The set and image/sampler handles are live and each binding
        // matches the common M2 pipeline descriptor layout.
        unsafe { device.update_descriptor_sets(&[write], &[]) };
    }
    Ok(())
}
