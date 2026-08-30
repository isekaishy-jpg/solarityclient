//! Logical device, queues, swapchain, and image-view lifetime ownership.

#![allow(unsafe_code)]

use ash::{Device, vk};
use solarity_asset::DecodedBlpTexture;

use crate::device::vulkan_frame::{FrameContext, present_blp};
use crate::device::vulkan_selection::SelectedAdapter;
use crate::device::{VulkanBootstrap, VulkanError};

/// Immutable evidence for the concrete Vulkan stack selected at startup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VulkanReport {
    device_name: String,
    api_version: u32,
    graphics_queue_family: u32,
    present_queue_family: u32,
    swapchain_image_count: usize,
    extent: (u32, u32),
    presented_texture_extent: Option<(u32, u32)>,
}

impl VulkanReport {
    /// Returns the Vulkan-reported physical adapter name.
    #[must_use]
    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// Returns the packed Vulkan API version reported by the adapter.
    #[must_use]
    pub const fn api_version(&self) -> u32 {
        self.api_version
    }

    /// Returns the queue family used for graphics submissions.
    #[must_use]
    pub const fn graphics_queue_family(&self) -> u32 {
        self.graphics_queue_family
    }

    /// Returns the queue family used for presentation.
    #[must_use]
    pub const fn present_queue_family(&self) -> u32 {
        self.present_queue_family
    }

    /// Returns the number of images owned by the swapchain.
    #[must_use]
    pub const fn swapchain_image_count(&self) -> usize {
        self.swapchain_image_count
    }

    /// Returns the physical swapchain width and height.
    #[must_use]
    pub const fn extent(&self) -> (u32, u32) {
        self.extent
    }

    /// Returns the decoded BLP extent after the first frame has been presented.
    #[must_use]
    pub const fn presented_texture_extent(&self) -> Option<(u32, u32)> {
        self.presented_texture_extent
    }
}

/// Sole owner of the initialized Vulkan presentation object graph.
pub struct VulkanRenderer {
    // Manual drop order is image views, swapchain, device, then the embedded
    // bootstrap's surface, instance, and loader.
    bootstrap: VulkanBootstrap,
    device: Device,
    allocator: Option<vk_mem::Allocator>,
    swapchain_loader: ash::khr::swapchain::Device,
    swapchain: vk::SwapchainKHR,
    swapchain_images: Vec<vk::Image>,
    image_views: Vec<vk::ImageView>,
    graphics_queue: vk::Queue,
    present_queue: vk::Queue,
    report: VulkanReport,
    is_idle: bool,
}

impl VulkanRenderer {
    /// Completes device and swapchain initialization for the attached surface.
    pub(super) fn start(
        bootstrap: VulkanBootstrap,
        requested_extent: (u32, u32),
        adapter_index: usize,
    ) -> Result<Self, VulkanError> {
        let selected = SelectedAdapter::select(&bootstrap, adapter_index)?;
        let device = create_device(&bootstrap, &selected)?;
        // SAFETY: Both family indices were queried from this physical device,
        // and queue zero was requested during logical-device creation.
        let graphics_queue = unsafe { device.get_device_queue(selected.graphics_family, 0) };
        // SAFETY: Same invariant as the graphics queue; families may be equal.
        let present_queue = unsafe { device.get_device_queue(selected.present_family, 0) };
        let swapchain_loader = ash::khr::swapchain::Device::new(&bootstrap.instance, &device);

        let extent = choose_extent(selected.surface_capabilities, requested_extent);
        let mut renderer = Self {
            bootstrap,
            device,
            allocator: None,
            swapchain_loader,
            swapchain: vk::SwapchainKHR::null(),
            swapchain_images: Vec::new(),
            image_views: Vec::new(),
            graphics_queue,
            present_queue,
            report: VulkanReport {
                // The selection remains borrowed until swapchain construction;
                // the report independently owns its diagnostic adapter label.
                device_name: selected.device_name.clone(),
                api_version: selected.api_version,
                graphics_queue_family: selected.graphics_family,
                present_queue_family: selected.present_family,
                swapchain_image_count: 0,
                extent: (extent.width, extent.height),
                presented_texture_extent: None,
            },
            is_idle: false,
        };
        renderer.create_allocator(selected.physical_device)?;
        renderer.create_swapchain(&selected, extent)?;
        Ok(renderer)
    }

    /// Returns immutable startup facts for diagnostics and compatibility tests.
    #[must_use]
    pub const fn report(&self) -> &VulkanReport {
        &self.report
    }

    /// Waits until all submitted device work is complete before owner teardown.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] if the driver reports device loss or another
    /// failure while waiting. Drop still attempts safe handle destruction.
    pub fn shutdown(&mut self) -> Result<(), VulkanError> {
        self.wait_idle()
    }

    /// Uploads and presents one decoded stock BLP before the window is revealed.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] when frame composition, staging, command
    /// submission, synchronization, or presentation fails.
    pub fn present_blp(&mut self, texture: &DecodedBlpTexture) -> Result<(), VulkanError> {
        let allocator = self.allocator.as_ref().ok_or_else(|| {
            VulkanError::operation("access Vulkan allocator", "allocator is unavailable")
        })?;
        present_blp(FrameContext {
            device: &self.device,
            allocator,
            swapchain_loader: &self.swapchain_loader,
            swapchain: self.swapchain,
            swapchain_images: &self.swapchain_images,
            graphics_queue: self.graphics_queue,
            present_queue: self.present_queue,
            graphics_queue_family: self.report.graphics_queue_family,
            frame_extent: self.report.extent,
            texture,
        })?;
        self.report.presented_texture_extent = Some((texture.width(), texture.height()));
        Ok(())
    }

    /// Creates the swapchain and one owned color view for each borrowed image.
    fn create_swapchain(
        &mut self,
        selected: &SelectedAdapter,
        extent: vk::Extent2D,
    ) -> Result<(), VulkanError> {
        let image_count = swapchain_image_count(selected.surface_capabilities);
        let queue_families = [selected.graphics_family, selected.present_family];
        let mut create_info = vk::SwapchainCreateInfoKHR::default()
            .surface(self.bootstrap.surface)
            .min_image_count(image_count)
            .image_format(selected.surface_format.format)
            .image_color_space(selected.surface_format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST)
            .pre_transform(selected.surface_capabilities.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(vk::PresentModeKHR::FIFO)
            .clipped(true);
        if selected.graphics_family == selected.present_family {
            create_info = create_info.image_sharing_mode(vk::SharingMode::EXCLUSIVE);
        } else {
            create_info = create_info
                .image_sharing_mode(vk::SharingMode::CONCURRENT)
                .queue_family_indices(&queue_families);
        }
        // SAFETY: All handles and slices in the create info belong to this
        // owner and remain alive for the duration of the call.
        self.swapchain = unsafe { self.swapchain_loader.create_swapchain(&create_info, None) }
            .map_err(|source| VulkanError::operation("create swapchain", source))?;
        // SAFETY: `self.swapchain` was created successfully by this loader.
        let images = unsafe { self.swapchain_loader.get_swapchain_images(self.swapchain) }
            .map_err(|source| VulkanError::operation("enumerate swapchain images", source))?;
        self.image_views.reserve(images.len());
        for &image in &images {
            let subresource_range = vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .base_mip_level(0)
                .level_count(1)
                .base_array_layer(0)
                .layer_count(1);
            let view_info = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(selected.surface_format.format)
                .components(vk::ComponentMapping::default())
                .subresource_range(subresource_range);
            // SAFETY: The swapchain image is live, the format matches its
            // creation format, and the view range addresses its sole mip/layer.
            let view = unsafe { self.device.create_image_view(&view_info, None) }
                .map_err(|source| VulkanError::operation("create swapchain image view", source))?;
            self.image_views.push(view);
        }
        self.swapchain_images = images;
        self.report.swapchain_image_count = self.image_views.len();
        Ok(())
    }

    /// Establishes the VMA owner before any device-local client resources exist.
    fn create_allocator(&mut self, physical_device: vk::PhysicalDevice) -> Result<(), VulkanError> {
        let mut create_info = vk_mem::AllocatorCreateInfo::new(
            &self.bootstrap.instance,
            &self.device,
            physical_device,
        );
        create_info.vulkan_api_version = vk::API_VERSION_1_3;
        // SAFETY: Instance, device, and physical device form the validated live
        // Vulkan object graph and outlive the stored allocator.
        let allocator = unsafe { vk_mem::Allocator::new(create_info) }
            .map_err(|source| VulkanError::operation("create Vulkan allocator", source))?;
        self.allocator = Some(allocator);
        Ok(())
    }

    /// Performs the one fallible teardown synchronization step idempotently.
    fn wait_idle(&mut self) -> Result<(), VulkanError> {
        if self.is_idle {
            return Ok(());
        }
        // SAFETY: The device is live and exclusively owned. Waiting does not
        // invalidate any handle and is permitted before teardown.
        unsafe { self.device.device_wait_idle() }
            .map_err(|source| VulkanError::operation("wait for device idle", source))?;
        self.is_idle = true;
        Ok(())
    }
}

impl Drop for VulkanRenderer {
    /// Releases Vulkan children in reverse dependency order.
    fn drop(&mut self) {
        let _idle_result = self.wait_idle();
        // SAFETY: Every handle was created by this device/loader and this owner
        // destroys each exactly once after attempting to idle the device.
        unsafe {
            for image_view in self.image_views.drain(..).rev() {
                self.device.destroy_image_view(image_view, None);
            }
            if self.swapchain != vk::SwapchainKHR::null() {
                self.swapchain_loader
                    .destroy_swapchain(self.swapchain, None);
                self.swapchain = vk::SwapchainKHR::null();
            }
            // VMA owns no Vulkan handles after all future allocated resources
            // have been destroyed; dropping it before the device is mandatory.
            drop(self.allocator.take());
            self.device.destroy_device(None);
        }
    }
}

/// Creates one queue per distinct family and enables required Vulkan 1.3 features.
fn create_device(
    bootstrap: &VulkanBootstrap,
    selected: &SelectedAdapter,
) -> Result<Device, VulkanError> {
    let priority = [1.0_f32];
    let mut family_indices = vec![selected.graphics_family];
    if selected.present_family != selected.graphics_family {
        family_indices.push(selected.present_family);
    }
    let queue_infos = family_indices
        .iter()
        .map(|family| {
            vk::DeviceQueueCreateInfo::default()
                .queue_family_index(*family)
                .queue_priorities(&priority)
        })
        .collect::<Vec<_>>();
    let extension_names = [ash::khr::swapchain::NAME.as_ptr()];
    let mut vulkan13 = vk::PhysicalDeviceVulkan13Features::default()
        .dynamic_rendering(true)
        .synchronization2(true);
    let create_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queue_infos)
        .enabled_extension_names(&extension_names)
        .push_next(&mut vulkan13);
    // SAFETY: Queue families and features were queried from this physical
    // device, and all create-info slices remain alive for the call.
    unsafe {
        bootstrap
            .instance
            .create_device(selected.physical_device, &create_info, None)
    }
    .map_err(|source| VulkanError::operation("create logical device", source))
}

/// Converts the surface's fixed or variable extent into a legal swapchain size.
fn choose_extent(capabilities: vk::SurfaceCapabilitiesKHR, requested: (u32, u32)) -> vk::Extent2D {
    if capabilities.current_extent.width != u32::MAX {
        return capabilities.current_extent;
    }
    vk::Extent2D {
        width: requested.0.clamp(
            capabilities.min_image_extent.width,
            capabilities.max_image_extent.width,
        ),
        height: requested.1.clamp(
            capabilities.min_image_extent.height,
            capabilities.max_image_extent.height,
        ),
    }
}

/// Requests one image beyond the surface minimum without exceeding its maximum.
fn swapchain_image_count(capabilities: vk::SurfaceCapabilitiesKHR) -> u32 {
    let preferred = capabilities.min_image_count.saturating_add(1);
    if capabilities.max_image_count == 0 {
        preferred
    } else {
        preferred.min(capabilities.max_image_count)
    }
}
