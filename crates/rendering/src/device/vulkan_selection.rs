//! Validation of the explicitly selected Vulkan adapter and surface contract.

#![allow(unsafe_code)]

use ash::vk;

use crate::device::{VulkanBootstrap, VulkanError};

/// Queue and surface facts fixed for logical-device and swapchain creation.
pub(super) struct SelectedAdapter {
    pub(super) physical_device: vk::PhysicalDevice,
    pub(super) device_name: String,
    pub(super) api_version: u32,
    pub(super) graphics_family: u32,
    pub(super) present_family: u32,
    pub(super) surface_format: vk::SurfaceFormatKHR,
    pub(super) depth_format: vk::Format,
    pub(super) uniform_buffer_alignment: vk::DeviceSize,
    pub(super) storage_buffer_alignment: vk::DeviceSize,
    pub(super) sampler_anisotropy: bool,
    pub(super) maximum_sampler_anisotropy: f32,
    pub(super) surface_capabilities: vk::SurfaceCapabilitiesKHR,
}

impl SelectedAdapter {
    /// Validates one explicitly indexed adapter without trying alternatives.
    pub(super) fn select(
        bootstrap: &VulkanBootstrap,
        adapter_index: usize,
    ) -> Result<Self, VulkanError> {
        // SAFETY: The bootstrap owns a live instance for the entire query.
        let adapters = unsafe { bootstrap.instance.enumerate_physical_devices() }
            .map_err(|source| VulkanError::operation("enumerate physical devices", source))?;
        let physical_device =
            adapters
                .get(adapter_index)
                .copied()
                .ok_or(VulkanError::AdapterUnavailable {
                    requested: adapter_index,
                    available: adapters.len(),
                })?;

        // SAFETY: `physical_device` was returned by this live instance.
        let properties = unsafe {
            bootstrap
                .instance
                .get_physical_device_properties(physical_device)
        };
        // SAFETY: The selected physical device belongs to this live instance.
        let features = unsafe {
            bootstrap
                .instance
                .get_physical_device_features(physical_device)
        };
        if properties.api_version < vk::API_VERSION_1_3 {
            return Err(VulkanError::AdapterApi {
                major: vk::api_version_major(properties.api_version),
                minor: vk::api_version_minor(properties.api_version),
                patch: vk::api_version_patch(properties.api_version),
            });
        }
        validate_block_compression(bootstrap, physical_device, &features)?;

        validate_swapchain_extension(bootstrap, physical_device)?;
        validate_vulkan13_features(bootstrap, physical_device)?;
        let (graphics_family, present_family) = select_queue_families(bootstrap, physical_device)?;
        let surface_format = select_surface_format(bootstrap, physical_device)?;
        let depth_format = select_depth_format(bootstrap, physical_device)?;
        validate_present_mode(bootstrap, physical_device)?;
        // SAFETY: The physical device and surface belong to the live bootstrap.
        let surface_capabilities = unsafe {
            bootstrap
                .surface_loader
                .get_physical_device_surface_capabilities(physical_device, bootstrap.surface)
        }
        .map_err(|source| VulkanError::operation("query surface capabilities", source))?;
        if !surface_capabilities
            .supported_composite_alpha
            .contains(vk::CompositeAlphaFlagsKHR::OPAQUE)
        {
            return Err(VulkanError::CompositeAlpha);
        }
        if !surface_capabilities
            .supported_usage_flags
            .contains(vk::ImageUsageFlags::COLOR_ATTACHMENT)
        {
            return Err(VulkanError::ColorAttachmentUsage);
        }
        if !surface_capabilities
            .supported_usage_flags
            .contains(vk::ImageUsageFlags::TRANSFER_DST)
        {
            return Err(VulkanError::TransferDestinationUsage);
        }
        if !surface_capabilities
            .supported_usage_flags
            .contains(vk::ImageUsageFlags::TRANSFER_SRC)
        {
            return Err(VulkanError::TransferSourceUsage);
        }

        let device_name = properties.device_name_as_c_str().map_or_else(
            |_source| "invalid Vulkan adapter name".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
        Ok(Self {
            physical_device,
            device_name,
            api_version: properties.api_version,
            graphics_family,
            present_family,
            surface_format,
            depth_format,
            uniform_buffer_alignment: properties.limits.min_uniform_buffer_offset_alignment,
            storage_buffer_alignment: properties.limits.min_storage_buffer_offset_alignment,
            sampler_anisotropy: features.sampler_anisotropy == vk::TRUE,
            maximum_sampler_anisotropy: properties.limits.max_sampler_anisotropy,
            surface_capabilities,
        })
    }
}

/// Requires direct sampled uploads for every authored WotLK DXT family.
fn validate_block_compression(
    bootstrap: &VulkanBootstrap,
    physical_device: vk::PhysicalDevice,
    features: &vk::PhysicalDeviceFeatures,
) -> Result<(), VulkanError> {
    if features.texture_compression_bc == vk::FALSE {
        return Err(VulkanError::TextureCompressionBc);
    }

    let required = vk::FormatFeatureFlags::SAMPLED_IMAGE
        | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR
        | vk::FormatFeatureFlags::TRANSFER_DST;
    let formats = [
        vk::Format::BC1_RGBA_UNORM_BLOCK,
        vk::Format::BC1_RGBA_SRGB_BLOCK,
        vk::Format::BC2_UNORM_BLOCK,
        vk::Format::BC2_SRGB_BLOCK,
        vk::Format::BC3_UNORM_BLOCK,
        vk::Format::BC3_SRGB_BLOCK,
    ];
    for format in formats {
        // SAFETY: The selected physical device belongs to the live instance.
        let properties = unsafe {
            bootstrap
                .instance
                .get_physical_device_format_properties(physical_device, format)
        };
        if !properties.optimal_tiling_features.contains(required) {
            return Err(VulkanError::TextureCompressionBcFormat);
        }
    }
    Ok(())
}

/// Requires the direct Vulkan counterpart to stock's 24-bit depth/stencil surface.
fn select_depth_format(
    bootstrap: &VulkanBootstrap,
    physical_device: vk::PhysicalDevice,
) -> Result<vk::Format, VulkanError> {
    let format = vk::Format::D24_UNORM_S8_UINT;
    // SAFETY: The selected physical device belongs to the live instance.
    let properties = unsafe {
        bootstrap
            .instance
            .get_physical_device_format_properties(physical_device, format)
    };
    if !properties
        .optimal_tiling_features
        .contains(vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT)
    {
        return Err(VulkanError::DepthStencilFormat);
    }
    Ok(format)
}

/// Requires the one device extension used by the presentation owner.
fn validate_swapchain_extension(
    bootstrap: &VulkanBootstrap,
    physical_device: vk::PhysicalDevice,
) -> Result<(), VulkanError> {
    // SAFETY: The selected physical device belongs to the live instance.
    let extensions = unsafe {
        bootstrap
            .instance
            .enumerate_device_extension_properties(physical_device)
    }
    .map_err(|source| VulkanError::operation("enumerate device extensions", source))?;
    let has_swapchain = extensions.iter().any(|extension| {
        extension
            .extension_name_as_c_str()
            .is_ok_and(|name| name == ash::khr::swapchain::NAME)
    });
    if !has_swapchain {
        return Err(VulkanError::SwapchainExtension);
    }
    Ok(())
}

/// Requires Vulkan 1.3 dynamic rendering and synchronization2 for later frames.
fn validate_vulkan13_features(
    bootstrap: &VulkanBootstrap,
    physical_device: vk::PhysicalDevice,
) -> Result<(), VulkanError> {
    let mut vulkan13 = vk::PhysicalDeviceVulkan13Features::default();
    let mut features = vk::PhysicalDeviceFeatures2::default().push_next(&mut vulkan13);
    // SAFETY: Ash initialized the output chain, and the device belongs to the instance.
    unsafe {
        bootstrap
            .instance
            .get_physical_device_features2(physical_device, &mut features);
    }
    if vulkan13.dynamic_rendering == vk::FALSE || vulkan13.synchronization2 == vk::FALSE {
        return Err(VulkanError::Vulkan13Features);
    }
    Ok(())
}

/// Prefers one combined graphics/present family, then permits distinct queues.
fn select_queue_families(
    bootstrap: &VulkanBootstrap,
    physical_device: vk::PhysicalDevice,
) -> Result<(u32, u32), VulkanError> {
    // SAFETY: The selected physical device belongs to the live instance.
    let properties = unsafe {
        bootstrap
            .instance
            .get_physical_device_queue_family_properties(physical_device)
    };
    let mut first_graphics = None;
    let mut first_present = None;
    for (index, properties) in properties.iter().enumerate() {
        let index = u32::try_from(index)
            .map_err(|source| VulkanError::operation("convert queue family index", source))?;
        let is_graphics = properties.queue_flags.contains(vk::QueueFlags::GRAPHICS);
        // SAFETY: The queue index comes from this physical device and the
        // surface remains live for the duration of the query.
        let is_present = unsafe {
            bootstrap
                .surface_loader
                .get_physical_device_surface_support(physical_device, index, bootstrap.surface)
        }
        .map_err(|source| VulkanError::operation("query queue surface support", source))?;
        if is_graphics && is_present {
            return Ok((index, index));
        }
        if is_graphics && first_graphics.is_none() {
            first_graphics = Some(index);
        }
        if is_present && first_present.is_none() {
            first_present = Some(index);
        }
    }
    first_graphics
        .zip(first_present)
        .ok_or(VulkanError::QueueFamilies)
}

/// Requires the BGRA8 layout corresponding to the stock D3D backbuffer family.
fn select_surface_format(
    bootstrap: &VulkanBootstrap,
    physical_device: vk::PhysicalDevice,
) -> Result<vk::SurfaceFormatKHR, VulkanError> {
    // SAFETY: The physical device and surface belong to the live bootstrap.
    let formats = unsafe {
        bootstrap
            .surface_loader
            .get_physical_device_surface_formats(physical_device, bootstrap.surface)
    }
    .map_err(|source| VulkanError::operation("query surface formats", source))?;
    formats
        .into_iter()
        .find(|format| {
            format.format == vk::Format::B8G8R8A8_UNORM
                && format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
        })
        .ok_or(VulkanError::SurfaceFormat)
}

/// Requires Vulkan's deterministic v-synchronized FIFO mode.
fn validate_present_mode(
    bootstrap: &VulkanBootstrap,
    physical_device: vk::PhysicalDevice,
) -> Result<(), VulkanError> {
    // SAFETY: The physical device and surface belong to the live bootstrap.
    let modes = unsafe {
        bootstrap
            .surface_loader
            .get_physical_device_surface_present_modes(physical_device, bootstrap.surface)
    }
    .map_err(|source| VulkanError::operation("query present modes", source))?;
    if !modes.contains(&vk::PresentModeKHR::FIFO) {
        return Err(VulkanError::PresentMode);
    }
    Ok(())
}
