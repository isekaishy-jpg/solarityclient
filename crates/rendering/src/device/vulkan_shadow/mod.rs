//! Frame-owned primary shadow maps and the original M2 caster pass.

mod environment_frame;
mod environment_images;
mod frame;
mod pipeline;
mod resource;

use ash::vk;

pub use environment_frame::{
    WorldEnvironmentM2Caster, WorldEnvironmentShadowFrame, WorldEnvironmentShadowPass,
    WorldEnvironmentWmoCaster,
};
pub(in crate::device) use environment_images::EnvironmentShadowImages;
pub use frame::WorldPrimaryShadowFrame;
pub(in crate::device) use pipeline::ShadowPipelines;
pub(in crate::device) use resource::ShadowFrameResources;

/// Identical receiver ABI for terrain, WMO, and ground-detail pipelines.
pub(in crate::device) fn receiver_bindings() -> [vk::DescriptorSetLayoutBinding<'static>; 5] {
    std::array::from_fn(|index| {
        vk::DescriptorSetLayoutBinding::default()
            .binding(index as u32)
            .descriptor_count(1)
            .descriptor_type(if index == 0 {
                vk::DescriptorType::UNIFORM_BUFFER
            } else {
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER
            })
            .stage_flags(if index == 0 {
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT
            } else {
                vk::ShaderStageFlags::FRAGMENT
            })
    })
}
