//! Exact-size descriptor allocation and renderer-lifetime pool ownership.

#![allow(unsafe_code)]

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use ash::{Device, vk};

use crate::device::VulkanError;
use crate::device::vulkan_m2_frame::PortraitRegistry;
use crate::device::vulkan_texture::BlpTextureRegistry;
use crate::device::vulkan_ui_glyph_texture::UiGlyphTextureRegistry;
use crate::device::vulkan_ui_sampler::UiSamplerRegistry;

use super::{UiSampledTexture, UiTextureImageHandle, UiTextureSetHandle, UiTextureSetInfo};

/// One live set owned transitively by a registry descriptor pool.
struct GpuUiTextureSet {
    handle: vk::DescriptorSet,
    pool: vk::DescriptorPool,
    info: UiTextureSetInfo,
}

/// Caches sampled pairs and owns exact-size allocation pools.
pub(in crate::device) struct UiTextureSetRegistry {
    registry_id: u64,
    handles: HashMap<UiSampledTexture, UiTextureSetHandle>,
    resources: HashMap<u32, GpuUiTextureSet>,
    next_slot: u32,
    pools: HashMap<vk::DescriptorPool, usize>,
}

impl Default for UiTextureSetRegistry {
    /// Assigns process-unique locality without guessing descriptor capacity.
    fn default() -> Self {
        static NEXT_REGISTRY_ID: AtomicU64 = AtomicU64::new(1);
        Self {
            registry_id: NEXT_REGISTRY_ID.fetch_add(1, Ordering::Relaxed),
            handles: HashMap::new(),
            resources: HashMap::new(),
            next_slot: 0,
            pools: HashMap::new(),
        }
    }
}

impl UiTextureSetRegistry {
    /// Number of live sampled-image descriptor resources.
    pub(in crate::device) fn resource_count(&self) -> usize {
        self.resources.len()
    }

    /// Resolves input order while allocating only unique new descriptor sets.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::device) fn prepare(
        &mut self,
        device: &Device,
        layout: vk::DescriptorSetLayout,
        textures: &BlpTextureRegistry,
        glyphs: &UiGlyphTextureRegistry,
        portraits: &PortraitRegistry,
        samplers: &UiSamplerRegistry,
        requested: &[UiSampledTexture],
    ) -> Result<Vec<UiTextureSetHandle>, VulkanError> {
        validate_resources(textures, glyphs, portraits, samplers, requested)?;
        let mut seen = HashSet::new();
        let pending = requested
            .iter()
            .copied()
            .filter(|pair| !self.handles.contains_key(pair) && seen.insert(*pair))
            .collect::<Vec<_>>();
        if !pending.is_empty() {
            self.allocate_batch(
                device, layout, textures, glyphs, portraits, samplers, &pending,
            )?;
        }
        requested
            .iter()
            .map(|pair| {
                self.handles.get(pair).copied().ok_or_else(|| {
                    VulkanError::operation(
                        "resolve UI texture descriptor set",
                        "prepared set is unavailable",
                    )
                })
            })
            .collect()
    }

    /// Returns stable diagnostics for one renderer-local descriptor identity.
    pub(in crate::device) fn info(&self, handle: UiTextureSetHandle) -> Option<UiTextureSetInfo> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources.get(&handle.slot).and_then(|resource| {
            (resource.handle != vk::DescriptorSet::null()).then_some(resource.info)
        })
    }

    /// Resolves one renderer-local identity to its live descriptor set.
    pub(in crate::device) fn raw(&self, handle: UiTextureSetHandle) -> Option<vk::DescriptorSet> {
        if handle.registry_id != self.registry_id {
            return None;
        }
        self.resources
            .get(&handle.slot)
            .map(|resource| resource.handle)
    }

    /// Detaches all sampled sets for a retired glyph page.
    pub(in crate::device) fn take_glyph(
        &mut self,
        glyph: crate::UiGlyphTextureHandle,
    ) -> (
        Vec<(vk::DescriptorPool, vk::DescriptorSet)>,
        Vec<vk::DescriptorPool>,
    ) {
        let mut slots = Vec::new();
        self.handles.retain(|pair, handle| {
            if pair.texture() == UiTextureImageHandle::Glyph(glyph) {
                slots.push(handle.slot);
                false
            } else {
                true
            }
        });
        let mut sets = Vec::new();
        let mut pools = Vec::new();
        for slot in slots {
            if let Some(resource) = self.resources.remove(&slot) {
                sets.push((resource.pool, resource.handle));
                if let Some(count) = self.pools.get_mut(&resource.pool) {
                    *count -= 1;
                    if *count == 0 {
                        self.pools.remove(&resource.pool);
                        pools.push(resource.pool);
                    }
                }
            }
        }
        (sets, pools)
    }

    /// Releases descriptor sets transitively through their owning pools.
    pub(in crate::device) fn destroy(&mut self, device: &Device) {
        self.handles.clear();
        self.resources.clear();
        // SAFETY: Pools belong to this device and descriptor use is retired.
        unsafe {
            for pool in self.pools.drain().map(|(pool, _)| pool) {
                device.destroy_descriptor_pool(pool, None);
            }
        }
    }

    /// Creates one pool sized to the exact unique sampled-pair batch.
    #[allow(clippy::too_many_arguments)]
    fn allocate_batch(
        &mut self,
        device: &Device,
        layout: vk::DescriptorSetLayout,
        textures: &BlpTextureRegistry,
        glyphs: &UiGlyphTextureRegistry,
        portraits: &PortraitRegistry,
        samplers: &UiSamplerRegistry,
        pending: &[UiSampledTexture],
    ) -> Result<(), VulkanError> {
        let first_slot = u32::try_from(self.next_slot as usize)
            .map_err(|_source| VulkanError::UiTextureSetCapacity)?;
        let added =
            u32::try_from(pending.len()).map_err(|_source| VulkanError::UiTextureSetCapacity)?;
        first_slot
            .checked_add(added)
            .ok_or(VulkanError::UiTextureSetCapacity)?;
        let pool_sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(added)];
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .flags(vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET)
            .max_sets(added)
            .pool_sizes(&pool_sizes);
        // SAFETY: Counts are nonzero because pending is nonempty.
        let pool = unsafe { device.create_descriptor_pool(&pool_info, None) }
            .map_err(|source| VulkanError::operation("create UI descriptor pool", source))?;
        let layouts = vec![layout; pending.len()];
        let allocate_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(pool)
            .set_layouts(&layouts);
        // SAFETY: The pool and compatible repeated layouts are live.
        let sets = match unsafe { device.allocate_descriptor_sets(&allocate_info) } {
            Ok(sets) => sets,
            Err(source) => {
                // SAFETY: No successfully allocated set escaped this failed pool.
                unsafe { device.destroy_descriptor_pool(pool, None) };
                return Err(VulkanError::operation(
                    "allocate UI texture descriptor sets",
                    source,
                ));
            }
        };
        for (pair, descriptor_set) in pending.iter().copied().zip(sets) {
            write_set(
                device,
                descriptor_set,
                textures,
                glyphs,
                portraits,
                samplers,
                pair,
            )?;
            let slot = self.next_slot;
            self.next_slot = slot
                .checked_add(1)
                .ok_or(VulkanError::UiTextureSetCapacity)?;
            let handle = UiTextureSetHandle {
                registry_id: self.registry_id,
                slot,
            };
            self.resources.insert(
                slot,
                GpuUiTextureSet {
                    handle: descriptor_set,
                    pool,
                    info: UiTextureSetInfo::new(pair),
                },
            );
            self.handles.insert(pair, handle);
        }
        self.pools.insert(pool, pending.len());
        Ok(())
    }
}

/// Rejects foreign image and sampler handles before allocating a pool.
fn validate_resources(
    textures: &BlpTextureRegistry,
    glyphs: &UiGlyphTextureRegistry,
    portraits: &PortraitRegistry,
    samplers: &UiSamplerRegistry,
    requested: &[UiSampledTexture],
) -> Result<(), VulkanError> {
    for pair in requested {
        match pair.texture() {
            UiTextureImageHandle::Blp(handle) if textures.view(handle).is_none() => {
                return Err(VulkanError::UnknownBlpTextureHandle);
            }
            UiTextureImageHandle::Glyph(handle) if glyphs.view(handle).is_none() => {
                return Err(VulkanError::UnknownUiGlyphTextureHandle);
            }
            UiTextureImageHandle::Portrait(handle) if portraits.view(handle).is_none() => {
                return Err(VulkanError::UnknownUiPortraitTextureHandle);
            }
            UiTextureImageHandle::Blp(_)
            | UiTextureImageHandle::Glyph(_)
            | UiTextureImageHandle::Portrait(_) => {}
        }
        if samplers.raw(pair.sampler()).is_none() {
            return Err(VulkanError::UnknownUiSamplerHandle);
        }
    }
    Ok(())
}

/// Writes the sole combined-image binding for one sampled pair.
fn write_set(
    device: &Device,
    descriptor_set: vk::DescriptorSet,
    textures: &BlpTextureRegistry,
    glyphs: &UiGlyphTextureRegistry,
    portraits: &PortraitRegistry,
    samplers: &UiSamplerRegistry,
    pair: UiSampledTexture,
) -> Result<(), VulkanError> {
    let image_info = vk::DescriptorImageInfo::default()
        .sampler(
            samplers
                .raw(pair.sampler())
                .ok_or(VulkanError::UnknownUiSamplerHandle)?,
        )
        .image_view(match pair.texture() {
            UiTextureImageHandle::Blp(handle) => textures
                .view(handle)
                .ok_or(VulkanError::UnknownBlpTextureHandle)?,
            UiTextureImageHandle::Glyph(handle) => glyphs
                .view(handle)
                .ok_or(VulkanError::UnknownUiGlyphTextureHandle)?,
            UiTextureImageHandle::Portrait(handle) => portraits
                .view(handle)
                .ok_or(VulkanError::UnknownUiPortraitTextureHandle)?,
        })
        .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
    let image_infos = [image_info];
    let write = vk::WriteDescriptorSet::default()
        .dst_set(descriptor_set)
        .dst_binding(0)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .image_info(&image_infos);
    // SAFETY: The set, image view, and sampler are live and match binding zero.
    unsafe { device.update_descriptor_sets(&[write], &[]) };
    Ok(())
}
