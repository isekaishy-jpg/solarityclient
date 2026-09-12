//! Immutable detail texture bindings shared by fence-retained mesh generations.

#![allow(unsafe_code)]

use std::collections::HashMap;
use std::sync::Arc;

use ash::{Device, vk};

use crate::{BlpTextureHandle, VulkanError, WorldModelBaseMip, WorldModelTextureFiltering};

use super::resource::DetailCreateContext;

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub(super) struct DetailMaterialKey {
    pub(super) texture: BlpTextureHandle,
    pub(super) filtering: WorldModelTextureFiltering,
    pub(super) base_mip: WorldModelBaseMip,
}

/// Registry ownership plus live meshes retain the sampler and its texture set.
pub(super) struct DetailMaterial {
    sampler: vk::Sampler,
    pool: vk::DescriptorPool,
    set: vk::DescriptorSet,
}

impl DetailMaterial {
    fn create(
        context: &DetailCreateContext<'_>,
        key: DetailMaterialKey,
        view: vk::ImageView,
    ) -> Result<Self, VulkanError> {
        let mut material = Self {
            sampler: vk::Sampler::null(),
            pool: vk::DescriptorPool::null(),
            set: vk::DescriptorSet::null(),
        };
        if let Err(error) = material.initialize(context, key, view) {
            material.destroy(context.device);
            return Err(error);
        }
        Ok(material)
    }

    fn initialize(
        &mut self,
        context: &DetailCreateContext<'_>,
        key: DetailMaterialKey,
        view: vk::ImageView,
    ) -> Result<(), VulkanError> {
        let requested = key.filtering.requested_anisotropy();
        let anisotropy = if context.anisotropy_supported {
            requested.min(context.maximum_anisotropy).max(1.0)
        } else {
            1.0
        };
        // 7D9990 calls 681BE0 with both wrapping bits enabled for detail textures.
        let info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(if key.filtering.uses_linear_mips() {
                vk::SamplerMipmapMode::LINEAR
            } else {
                vk::SamplerMipmapMode::NEAREST
            })
            .address_mode_u(vk::SamplerAddressMode::REPEAT)
            .address_mode_v(vk::SamplerAddressMode::REPEAT)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .min_lod(key.base_mip.level())
            .max_lod(vk::LOD_CLAMP_NONE)
            .anisotropy_enable(anisotropy > 1.0)
            .max_anisotropy(anisotropy);
        // SAFETY: Anisotropy is capped to the enabled feature and device limit.
        self.sampler = unsafe { context.device.create_sampler(&info, None) }
            .map_err(|source| VulkanError::operation("create detail sampler", source))?;
        let sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: 1,
        }];
        let info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(1)
            .pool_sizes(&sizes);
        // SAFETY: The immutable material contains exactly one sampled texture.
        self.pool = unsafe { context.device.create_descriptor_pool(&info, None) }
            .map_err(|source| VulkanError::operation("create detail descriptor pool", source))?;
        let layouts = [context.descriptor];
        let info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(self.pool)
            .set_layouts(&layouts);
        // SAFETY: The detail pipeline owns the compatible live descriptor layout.
        self.set = unsafe { context.device.allocate_descriptor_sets(&info) }
            .map_err(|source| VulkanError::operation("allocate detail texture set", source))?
            .into_iter()
            .next()
            .ok_or(VulkanError::WorldFrameCapacity)?;
        let images = [vk::DescriptorImageInfo::default()
            .sampler(self.sampler)
            .image_view(view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let writes = [vk::WriteDescriptorSet::default()
            .dst_set(self.set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&images)];
        // SAFETY: The resolved view is resident and this set has never been submitted.
        unsafe { context.device.update_descriptor_sets(&writes, &[]) };
        Ok(())
    }

    pub(super) const fn set(&self) -> vk::DescriptorSet {
        self.set
    }

    fn destroy(&mut self, device: &Device) {
        // SAFETY: Failed creation is unsubmitted; ordinary retirement requires
        // exclusive registry ownership after all fence-retained meshes leave.
        unsafe {
            if self.pool != vk::DescriptorPool::null() {
                device.destroy_descriptor_pool(self.pool, None);
            }
            if self.sampler != vk::Sampler::null() {
                device.destroy_sampler(self.sampler, None);
            }
        }
        self.pool = vk::DescriptorPool::null();
        self.sampler = vk::Sampler::null();
        self.set = vk::DescriptorSet::null();
    }
}

#[derive(Default)]
pub(super) struct DetailMaterialRegistry {
    materials: HashMap<DetailMaterialKey, Arc<DetailMaterial>>,
}

impl DetailMaterialRegistry {
    pub(super) fn acquire(
        &mut self,
        context: &DetailCreateContext<'_>,
        key: DetailMaterialKey,
    ) -> Result<Arc<DetailMaterial>, VulkanError> {
        // Preserve handle validation even when a material already exists.
        let view = context
            .textures
            .view(key.texture)
            .ok_or(VulkanError::UnknownBlpTextureHandle)?;
        if let Some(material) = self.materials.get(&key) {
            return Ok(Arc::clone(material));
        }
        let material = Arc::new(DetailMaterial::create(context, key, view)?);
        self.materials.insert(key, Arc::clone(&material));
        Ok(material)
    }

    /// Runs after mesh arrivals, so exchanged generations can keep their bank.
    pub(super) fn retire_unused(&mut self, device: &Device) {
        self.materials.retain(|_, material| {
            if let Some(material) = Arc::get_mut(material) {
                material.destroy(device);
                false
            } else {
                true
            }
        });
    }

    /// All mesh owners must have been released after world-slot retirement.
    pub(super) fn destroy(&mut self, device: &Device) {
        self.retire_unused(device);
        debug_assert!(self.materials.is_empty());
    }
}
