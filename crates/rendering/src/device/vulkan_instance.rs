//! Vulkan loader and instance bootstrap before SDL surface attachment.

#![allow(unsafe_code)]

use std::ffi::CString;

use ash::{Entry, Instance, vk};

use crate::device::{VulkanError, VulkanPresentMode, VulkanRenderer};

/// Instance owner used during the SDL/Vulkan two-stage initialization seam.
pub struct VulkanBootstrap {
    pub(super) _entry: Entry,
    pub(super) instance: Instance,
    pub(super) surface_loader: ash::khr::surface::Instance,
    pub(super) surface: vk::SurfaceKHR,
}

impl VulkanBootstrap {
    /// Loads Vulkan 1.3 and enables exactly the instance extensions SDL requires.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] when the loader, API level, extension encoding,
    /// or instance creation contract fails.
    pub fn start(required_extensions: &[String]) -> Result<Self, VulkanError> {
        // SAFETY: `Entry` owns the dynamically loaded Vulkan library and is
        // retained longer than every function table created from it.
        let entry = unsafe { Entry::load() }.map_err(|source| VulkanError::Load {
            message: source.to_string(),
        })?;
        // SAFETY: The loaded entry remains alive and the call only writes the
        // loader-supported version into Ash-managed storage.
        let loader_version = unsafe { entry.try_enumerate_instance_version() }
            .map_err(|source| VulkanError::operation("enumerate instance version", source))?
            .unwrap_or(vk::API_VERSION_1_0);
        if loader_version < vk::API_VERSION_1_3 {
            return Err(VulkanError::UnsupportedApi {
                major: vk::api_version_major(loader_version),
                minor: vk::api_version_minor(loader_version),
                patch: vk::api_version_patch(loader_version),
            });
        }

        let extension_names = required_extensions
            .iter()
            .map(|extension| {
                CString::new(extension.as_str()).map_err(|_source| VulkanError::InvalidExtension {
                    extension: extension.clone(),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let extension_pointers = extension_names
            .iter()
            .map(|extension| extension.as_ptr())
            .collect::<Vec<_>>();
        let application_info = vk::ApplicationInfo::default()
            .application_name(c"Solarity")
            .application_version(vk::make_api_version(0, 0, 1, 0))
            .engine_name(c"Solarity")
            .engine_version(vk::make_api_version(0, 0, 1, 0))
            .api_version(vk::API_VERSION_1_3);
        let create_info = vk::InstanceCreateInfo::default()
            .application_info(&application_info)
            .enabled_extension_names(&extension_pointers);
        // SAFETY: All pointers in `create_info` refer to local C strings that
        // remain alive for the duration of the call. No custom allocator is used.
        let instance = unsafe { entry.create_instance(&create_info, None) }
            .map_err(|source| VulkanError::operation("create instance", source))?;
        let surface_loader = ash::khr::surface::Instance::new(&entry, &instance);

        Ok(Self {
            _entry: entry,
            instance,
            surface_loader,
            surface: vk::SurfaceKHR::null(),
        })
    }

    /// Returns the instance handle SDL needs to create its native surface.
    #[must_use]
    pub fn instance_handle(&self) -> vk::Instance {
        self.instance.handle()
    }

    /// Transfers a surface created for this exact instance into renderer ownership.
    ///
    /// # Safety
    ///
    /// `surface` must be a live `VkSurfaceKHR` created from
    /// [`Self::instance_handle`], and ownership must not be retained elsewhere.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] if the explicit adapter or surface cannot satisfy
    /// the required Vulkan 1.3 device and swapchain contract. The surface and
    /// instance are still destroyed on every failure path.
    pub unsafe fn attach_surface(
        mut self,
        surface: vk::SurfaceKHR,
        requested_extent: (u32, u32),
        adapter_index: usize,
    ) -> Result<VulkanRenderer, VulkanError> {
        self.surface = surface;
        VulkanRenderer::start(
            self,
            requested_extent,
            adapter_index,
            VulkanPresentMode::Synchronized,
        )
    }

    /// Transfers a surface and selects the requested stock VSync policy.
    ///
    /// # Safety
    ///
    /// `surface` must be a live `VkSurfaceKHR` created from
    /// [`Self::instance_handle`], and ownership must not be retained elsewhere.
    ///
    /// # Errors
    ///
    /// Returns [`VulkanError`] under the same adapter and surface failures as
    /// [`Self::attach_surface`].
    pub unsafe fn attach_surface_with_present_mode(
        mut self,
        surface: vk::SurfaceKHR,
        requested_extent: (u32, u32),
        adapter_index: usize,
        present_mode: VulkanPresentMode,
    ) -> Result<VulkanRenderer, VulkanError> {
        self.surface = surface;
        VulkanRenderer::start(self, requested_extent, adapter_index, present_mode)
    }
}

impl Drop for VulkanBootstrap {
    /// Destroys the SDL-created surface before its parent Vulkan instance.
    fn drop(&mut self) {
        // SAFETY: This owner is the sole owner of both handles. Renderer-owned
        // children are destroyed before this embedded bootstrap is dropped.
        unsafe {
            if self.surface != vk::SurfaceKHR::null() {
                self.surface_loader.destroy_surface(self.surface, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}
