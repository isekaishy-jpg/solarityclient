//! Public liquid resource operations composed through renderer-owned Vulkan state.

use crate::device::vulkan_liquid::{LiquidDrawMaterial, LiquidMeshHandle, LiquidPreparedDraw};
use crate::device::vulkan_mesh::MeshUploadContext;
use crate::device::{BlpTextureHandle, VulkanError, VulkanRenderer};
use crate::{LiquidRenderVertex, LiquidShaderUniform};

impl VulkanRenderer {
    /// Validates resident liquid resources before publishing one frame's draw packet.
    ///
    /// # Errors
    /// Returns [`VulkanError`] for foreign or retired mesh/texture handles.
    pub fn prepare_liquid_draw(
        &self,
        mesh: LiquidMeshHandle,
        material: LiquidDrawMaterial,
        surface: BlpTextureHandle,
        uniform: LiquidShaderUniform,
    ) -> Result<LiquidPreparedDraw, VulkanError> {
        if self.liquid_meshes.raw(mesh).is_none() {
            return Err(VulkanError::operation(
                "prepare liquid draw",
                "unknown liquid mesh handle",
            ));
        }
        if self.blp_textures.view(surface).is_none() {
            return Err(VulkanError::UnknownBlpTextureHandle);
        }
        Ok(LiquidPreparedDraw::new(mesh, material, surface, uniform))
    }

    /// Uploads one retained liquid triangle strip without a CPU wait for transfer.
    ///
    /// The caller retains the returned handle across frames and retires it when
    /// its terrain layer or WMO placement leaves residency. Draw submission on
    /// the same graphics queue orders geometry reads after the upload.
    ///
    /// # Errors
    /// Returns [`VulkanError`] for empty or invalid indexed geometry, exhausted
    /// identities, or Vulkan allocation and submission failures.
    pub fn upload_liquid_mesh(
        &mut self,
        vertices: &[LiquidRenderVertex],
        indices: &[u16],
    ) -> Result<LiquidMeshHandle, VulkanError> {
        let allocator = self.allocator.as_ref().ok_or_else(|| {
            VulkanError::operation("access Vulkan allocator", "allocator is unavailable")
        })?;
        let handle = self.liquid_meshes.upload(
            MeshUploadContext {
                device: &self.device,
                allocator,
                graphics_queue: self.graphics_queue,
                graphics_queue_family: self.report.graphics_queue_family,
            },
            vertices,
            indices,
        )?;
        self.is_idle = false;
        Ok(handle)
    }

    /// Invalidates streamed liquid handles and retires their last GPU use asynchronously.
    ///
    /// Already retired and foreign-renderer handles have no effect.
    ///
    /// # Errors
    /// Returns [`VulkanError`] for fence creation, polling, or submission failures.
    pub fn retire_liquid_meshes(
        &mut self,
        handles: &[LiquidMeshHandle],
    ) -> Result<(), VulkanError> {
        let allocator = self.allocator.as_ref().ok_or_else(|| {
            VulkanError::operation("access Vulkan allocator", "allocator is unavailable")
        })?;
        self.liquid_meshes.retire(
            MeshUploadContext {
                device: &self.device,
                allocator,
                graphics_queue: self.graphics_queue,
                graphics_queue_family: self.report.graphics_queue_family,
            },
            handles,
        )?;
        self.is_idle = false;
        Ok(())
    }
}
