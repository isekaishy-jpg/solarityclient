//! Exact-size descriptor allocation and fixed terrain sampler ownership.

#![allow(unsafe_code)]

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use ash::{Device, vk};

use crate::device::VulkanError;
use crate::device::vulkan_terrain_material::TerrainMaterialRegistry;
use crate::device::vulkan_texture::BlpTextureRegistry;

use super::{TerrainTextureSet, TerrainTextureSetHandle, TerrainTextureSetInfo};

/// A batch remains allocated until every descriptor in it has retired.
struct TerrainDescriptorPool {
    handle: vk::DescriptorPool,
    slots: std::ops::Range<u32>,
}

struct GpuTerrainTextureSet {
    handle: vk::DescriptorSet,
    info: TerrainTextureSetInfo,
}

pub(in crate::device) struct TerrainTextureSetRegistry {
    registry_id: u64,
    handles: HashMap<TerrainTextureSet, TerrainTextureSetHandle>,
    resources: HashMap<u32, GpuTerrainTextureSet>,
    next_slot: u32,
    pools: Vec<TerrainDescriptorPool>,
    atlas_sampler: vk::Sampler,
    diffuse_sampler: vk::Sampler,
}

impl Default for TerrainTextureSetRegistry {
    fn default() -> Self {
        static NEXT_REGISTRY_ID: AtomicU64 = AtomicU64::new(1);
        Self {
            registry_id: NEXT_REGISTRY_ID.fetch_add(1, Ordering::Relaxed),
            handles: HashMap::new(),
            resources: HashMap::new(),
            next_slot: 0,
            pools: Vec::new(),
            atlas_sampler: vk::Sampler::null(),
            diffuse_sampler: vk::Sampler::null(),
        }
    }
}

impl TerrainTextureSetRegistry {
    pub(in crate::device) fn prepare(
        &mut self,
        device: &Device,
        layout: vk::DescriptorSetLayout,
        materials: &TerrainMaterialRegistry,
        textures: &BlpTextureRegistry,
        requested: &[TerrainTextureSet],
    ) -> Result<Vec<TerrainTextureSetHandle>, VulkanError> {
        validate_resources(materials, textures, requested)?;
        let mut seen = HashSet::new();
        let pending = requested
            .iter()
            .filter(|set| !self.handles.contains_key(*set) && seen.insert((*set).clone()))
            .cloned()
            .collect::<Vec<_>>();
        if !pending.is_empty() {
            self.ensure_samplers(device)?;
            self.allocate_batch(device, layout, materials, textures, &pending)?;
        }
        requested
            .iter()
            .map(|set| {
                self.handles.get(set).copied().ok_or_else(|| {
                    VulkanError::operation(
                        "resolve terrain texture descriptor set",
                        "prepared set is unavailable",
                    )
                })
            })
            .collect()
    }

    pub(in crate::device) fn info(
        &self,
        handle: TerrainTextureSetHandle,
    ) -> Option<TerrainTextureSetInfo> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(&handle.slot)
            .map(|resource| resource.info)
    }

    pub(in crate::device) fn matches(
        &self,
        handle: TerrainTextureSetHandle,
        requested: &TerrainTextureSet,
    ) -> bool {
        handle.registry_id == self.registry_id
            && self
                .handles
                .get(requested)
                .is_some_and(|found| *found == handle)
    }

    pub(in crate::device) fn raw(
        &self,
        handle: TerrainTextureSetHandle,
    ) -> Option<vk::DescriptorSet> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(&handle.slot)
            .map(|resource| resource.handle)
    }

    /// Invalidates every descriptor using retired atlases and transfers empty pools.
    /// A mixed-atlas allocation remains owned until its final live set departs.
    pub(in crate::device) fn take_materials(
        &mut self,
        materials: &[crate::device::TerrainMaterialHandle],
    ) -> Vec<vk::DescriptorPool> {
        self.handles.retain(|key, handle| {
            if materials.contains(&key.material()) {
                self.resources.remove(&handle.slot);
                false
            } else {
                true
            }
        });
        let mut pools = Vec::new();
        self.pools.retain(|pool| {
            if pool
                .slots
                .clone()
                .any(|slot| self.resources.contains_key(&slot))
            {
                true
            } else {
                pools.push(pool.handle);
                false
            }
        });
        pools
    }

    pub(in crate::device) fn destroy(&mut self, device: &Device) {
        self.handles.clear();
        self.resources.clear();
        // SAFETY: All descriptor use is retired and handles are uniquely owned.
        unsafe {
            for pool in self.pools.drain(..).rev() {
                device.destroy_descriptor_pool(pool.handle, None);
            }
            if self.diffuse_sampler != vk::Sampler::null() {
                device.destroy_sampler(self.diffuse_sampler, None);
                self.diffuse_sampler = vk::Sampler::null();
            }
            if self.atlas_sampler != vk::Sampler::null() {
                device.destroy_sampler(self.atlas_sampler, None);
                self.atlas_sampler = vk::Sampler::null();
            }
        }
    }

    fn ensure_samplers(&mut self, device: &Device) -> Result<(), VulkanError> {
        if self.atlas_sampler != vk::Sampler::null() {
            return Ok(());
        }
        let atlas_info = sampler_info(vk::SamplerAddressMode::CLAMP_TO_EDGE, 0.0);
        // SAFETY: The self-contained create info requests core sampler state.
        self.atlas_sampler = unsafe { device.create_sampler(&atlas_info, None) }
            .map_err(|source| VulkanError::operation("create terrain atlas sampler", source))?;
        let diffuse_info = sampler_info(vk::SamplerAddressMode::REPEAT, vk::LOD_CLAMP_NONE);
        // SAFETY: Same invariant as the atlas sampler.
        self.diffuse_sampler = match unsafe { device.create_sampler(&diffuse_info, None) } {
            Ok(sampler) => sampler,
            Err(source) => {
                // SAFETY: The atlas sampler was just created and has no users.
                unsafe { device.destroy_sampler(self.atlas_sampler, None) };
                self.atlas_sampler = vk::Sampler::null();
                return Err(VulkanError::operation(
                    "create terrain diffuse sampler",
                    source,
                ));
            }
        };
        Ok(())
    }

    fn allocate_batch(
        &mut self,
        device: &Device,
        layout: vk::DescriptorSetLayout,
        materials: &TerrainMaterialRegistry,
        textures: &BlpTextureRegistry,
        pending: &[TerrainTextureSet],
    ) -> Result<(), VulkanError> {
        let first = self.next_slot;
        let added = u32::try_from(pending.len())
            .map_err(|_source| VulkanError::TerrainTextureSetCapacity)?;
        let next_slot = first
            .checked_add(added)
            .ok_or(VulkanError::TerrainTextureSetCapacity)?;
        // The common layout reserves the atlas plus four diffuse bindings even
        // when a smaller shader variant statically consumes fewer descriptors.
        let descriptor_count = added
            .checked_mul(5)
            .ok_or(VulkanError::TerrainTextureSetCapacity)?;
        let pool_sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(descriptor_count)];
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(added)
            .pool_sizes(&pool_sizes);
        // SAFETY: This path receives a nonempty batch with checked counts.
        let pool = unsafe { device.create_descriptor_pool(&pool_info, None) }
            .map_err(|source| VulkanError::operation("create terrain descriptor pool", source))?;
        let layouts = vec![layout; pending.len()];
        let allocation = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(pool)
            .set_layouts(&layouts);
        // SAFETY: Pool and compatible layouts remain live for the call.
        let descriptor_sets = match unsafe { device.allocate_descriptor_sets(&allocation) } {
            Ok(sets) => sets,
            Err(source) => {
                // SAFETY: No successful descriptor allocation escaped.
                unsafe { device.destroy_descriptor_pool(pool, None) };
                return Err(VulkanError::operation(
                    "allocate terrain descriptor sets",
                    source,
                ));
            }
        };
        for (slot, (key, descriptor_set)) in
            (first..next_slot).zip(pending.iter().cloned().zip(descriptor_sets))
        {
            write_set(
                device,
                descriptor_set,
                materials,
                textures,
                self.atlas_sampler,
                self.diffuse_sampler,
                &key,
            )?;
            let handle = TerrainTextureSetHandle {
                registry_id: self.registry_id,
                slot,
            };
            self.resources.insert(
                slot,
                GpuTerrainTextureSet {
                    handle: descriptor_set,
                    info: TerrainTextureSetInfo::new(key.layer_count()),
                },
            );
            self.handles.insert(key, handle);
        }
        self.next_slot = next_slot;
        self.pools.push(TerrainDescriptorPool {
            handle: pool,
            slots: first..next_slot,
        });
        Ok(())
    }
}

fn sampler_info(address: vk::SamplerAddressMode, max_lod: f32) -> vk::SamplerCreateInfo<'static> {
    vk::SamplerCreateInfo::default()
        .mag_filter(vk::Filter::LINEAR)
        .min_filter(vk::Filter::LINEAR)
        .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
        .address_mode_u(address)
        .address_mode_v(address)
        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .min_lod(0.0)
        .max_lod(max_lod)
        .anisotropy_enable(false)
        .compare_enable(false)
        .unnormalized_coordinates(false)
}

fn validate_resources(
    materials: &TerrainMaterialRegistry,
    textures: &BlpTextureRegistry,
    sets: &[TerrainTextureSet],
) -> Result<(), VulkanError> {
    for set in sets {
        if materials.view(set.material()).is_none() {
            return Err(VulkanError::UnknownTerrainMaterialHandle);
        }
        if set
            .layers()
            .iter()
            .any(|handle| textures.view(*handle).is_none())
        {
            return Err(VulkanError::UnknownBlpTextureHandle);
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_set(
    device: &Device,
    descriptor_set: vk::DescriptorSet,
    materials: &TerrainMaterialRegistry,
    textures: &BlpTextureRegistry,
    atlas_sampler: vk::Sampler,
    diffuse_sampler: vk::Sampler,
    set: &TerrainTextureSet,
) -> Result<(), VulkanError> {
    let atlas_view = materials
        .view(set.material())
        .ok_or(VulkanError::UnknownTerrainMaterialHandle)?;
    let atlas = [vk::DescriptorImageInfo::default()
        .sampler(atlas_sampler)
        .image_view(atlas_view)
        .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
    let atlas_write = vk::WriteDescriptorSet::default()
        .dst_set(descriptor_set)
        .dst_binding(0)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .image_info(&atlas);
    // SAFETY: Binding zero matches the live terrain material layout.
    unsafe { device.update_descriptor_sets(&[atlas_write], &[]) };
    for (index, texture) in set.layers().iter().copied().enumerate() {
        let image = [vk::DescriptorImageInfo::default()
            .sampler(diffuse_sampler)
            .image_view(
                textures
                    .view(texture)
                    .ok_or(VulkanError::UnknownBlpTextureHandle)?,
            )
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let binding = u32::try_from(index + 1)
            .map_err(|source| VulkanError::operation("convert terrain texture binding", source))?;
        let write = vk::WriteDescriptorSet::default()
            .dst_set(descriptor_set)
            .dst_binding(binding)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&image);
        // SAFETY: Each statically consumed binding matches the common layout.
        unsafe { device.update_descriptor_sets(&[write], &[]) };
    }
    Ok(())
}
