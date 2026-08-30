//! Authored-mip decoding and one-shot transfer to a sampled Vulkan image.

#![allow(unsafe_code)]

use ash::{Device, vk};
use solarity_asset::{AssetError, BlpTextureSource};
use vk_mem::Alloc;

use crate::device::VulkanError;

use super::status::BlpTextureUploadError;
use super::types::{BlpColorSpace, BlpTextureResourceInfo, BlpTextureSourceKind};

/// Borrowed renderer objects required for one synchronous texture transfer.
#[derive(Clone, Copy)]
pub(in crate::device) struct TextureUploadContext<'a> {
    pub(in crate::device) device: &'a Device,
    pub(in crate::device) allocator: &'a vk_mem::Allocator,
    pub(in crate::device) graphics_queue: vk::Queue,
    pub(in crate::device) graphics_queue_family: u32,
}

/// One decoded mip's byte offset and exact two-dimensional extent.
struct DecodedMip {
    offset: vk::DeviceSize,
    width: u32,
    height: u32,
}

/// Device-local image/view pair owned by the renderer texture registry.
pub(super) struct GpuBlpTexture {
    image: GpuSampledImage,
    info: BlpTextureResourceInfo,
}

/// Device-local sampled image storage shared by typed texture registries.
pub(in crate::device) struct GpuSampledImage {
    image: vk::Image,
    allocation: Option<vk_mem::Allocation>,
    view: vk::ImageView,
}

impl GpuBlpTexture {
    /// Returns stable allocation diagnostics without exposing Vulkan objects.
    pub(super) const fn info(&self) -> &BlpTextureResourceInfo {
        &self.info
    }

    /// Returns the live sampled image view for descriptor construction.
    pub(super) const fn view(&self) -> vk::ImageView {
        self.image.view()
    }

    /// Destroys the child view before its allocated parent image.
    pub(super) fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        self.image.destroy(device, allocator);
    }
}

impl GpuSampledImage {
    /// Returns the live sampled view for descriptor construction.
    pub(in crate::device) const fn view(&self) -> vk::ImageView {
        self.view
    }

    /// Destroys the child view before its allocated parent image.
    pub(in crate::device) fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        // SAFETY: Both handles are uniquely owned and no command references
        // them after their registry's parent renderer has gone idle.
        unsafe {
            if self.view != vk::ImageView::null() {
                device.destroy_image_view(self.view, None);
                self.view = vk::ImageView::null();
            }
            if let Some(mut allocation) = self.allocation.take() {
                allocator.destroy_image(self.image, &mut allocation);
                self.image = vk::Image::null();
            }
        }
    }
}

/// Guard ensuring partially initialized permanent resources never leak.
struct TextureGuard<'a> {
    device: &'a Device,
    allocator: &'a vk_mem::Allocator,
    texture: Option<GpuSampledImage>,
}

impl TextureGuard<'_> {
    /// Transfers ownership after every upload and view operation succeeds.
    fn finish(mut self) -> Result<GpuSampledImage, VulkanError> {
        self.texture.take().ok_or_else(|| {
            VulkanError::operation("finish sampled image upload", "image is unavailable")
        })
    }
}

impl Drop for TextureGuard<'_> {
    /// Covers failures after image allocation and before registry insertion.
    fn drop(&mut self) {
        if let Some(texture) = self.texture.as_mut() {
            texture.destroy(self.device, self.allocator);
        }
    }
}

/// Host-visible staging allocation and one transient submission lifetime.
struct TextureTransfer<'a> {
    context: TextureUploadContext<'a>,
    staging_buffer: vk::Buffer,
    staging_allocation: Option<vk_mem::Allocation>,
    command_pool: vk::CommandPool,
    fence: vk::Fence,
}

impl<'a> TextureTransfer<'a> {
    /// Allocates and fills exact-size sequential upload memory.
    fn create(context: TextureUploadContext<'a>, bytes: &[u8]) -> Result<Self, VulkanError> {
        let size = u64::try_from(bytes.len())
            .map_err(|source| VulkanError::operation("convert BLP staging size", source))?;
        let buffer_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let allocation_info = vk_mem::AllocationCreateInfo {
            flags: vk_mem::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
            usage: vk_mem::MemoryUsage::AutoPreferHost,
            required_flags: vk::MemoryPropertyFlags::HOST_VISIBLE,
            ..Default::default()
        };
        // SAFETY: VMA receives initialized create infos and owns the returned
        // binding until this transfer destroys it.
        let (staging_buffer, allocation) = unsafe {
            context
                .allocator
                .create_buffer(&buffer_info, &allocation_info)
        }
        .map_err(|source| VulkanError::operation("create BLP staging buffer", source))?;
        let mut transfer = Self {
            context,
            staging_buffer,
            staging_allocation: Some(allocation),
            command_pool: vk::CommandPool::null(),
            fence: vk::Fence::null(),
        };
        transfer.write(bytes)?;
        transfer.create_submission_objects()?;
        Ok(transfer)
    }

    /// Copies all decoded mip bytes and flushes noncoherent host memory.
    fn write(&mut self, bytes: &[u8]) -> Result<(), VulkanError> {
        let allocation = self.staging_allocation.as_mut().ok_or_else(|| {
            VulkanError::operation("access BLP staging allocation", "allocation is unavailable")
        })?;
        // SAFETY: The allocation is host-visible and exactly covers `bytes`.
        let destination = unsafe { self.context.allocator.map_memory(allocation) }
            .map_err(|source| VulkanError::operation("map BLP staging buffer", source))?;
        // SAFETY: Both pointers are valid for the exact nonoverlapping byte count.
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), destination, bytes.len()) };
        let flush = self
            .context
            .allocator
            .flush_allocation(allocation, 0, vk::WHOLE_SIZE)
            .map_err(|source| VulkanError::operation("flush BLP staging buffer", source));
        // SAFETY: This balances the successful map on both success and failure.
        unsafe { self.context.allocator.unmap_memory(allocation) };
        flush
    }

    /// Creates one transient command pool and its retirement fence.
    fn create_submission_objects(&mut self) -> Result<(), VulkanError> {
        let pool_info = vk::CommandPoolCreateInfo::default()
            .flags(vk::CommandPoolCreateFlags::TRANSIENT)
            .queue_family_index(self.context.graphics_queue_family);
        // SAFETY: The selected graphics family is enabled on this device.
        self.command_pool = unsafe { self.context.device.create_command_pool(&pool_info, None) }
            .map_err(|source| VulkanError::operation("create BLP command pool", source))?;
        // SAFETY: Default fence creation borrows no external state.
        self.fence = unsafe {
            self.context
                .device
                .create_fence(&vk::FenceCreateInfo::default(), None)
        }
        .map_err(|source| VulkanError::operation("create BLP transfer fence", source))?;
        Ok(())
    }

    /// Allocates the sole primary command buffer from the transient pool.
    fn command_buffer(&self) -> Result<vk::CommandBuffer, VulkanError> {
        let allocate_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(self.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        // SAFETY: The pool is live and its command buffers do not escape.
        unsafe { self.context.device.allocate_command_buffers(&allocate_info) }
            .map_err(|source| VulkanError::operation("allocate BLP command buffer", source))?
            .first()
            .copied()
            .ok_or_else(|| {
                VulkanError::operation("allocate BLP command buffer", "driver returned none")
            })
    }

    /// Submits and synchronously retires the transfer source.
    fn submit_and_wait(&self, command_buffer: vk::CommandBuffer) -> Result<(), VulkanError> {
        let command_info = vk::CommandBufferSubmitInfo::default().command_buffer(command_buffer);
        let command_infos = [command_info];
        let submit_info = vk::SubmitInfo2::default().command_buffer_infos(&command_infos);
        // SAFETY: The executable command and every resource it references stay
        // live through the fence wait immediately below.
        unsafe {
            self.context.device.queue_submit2(
                self.context.graphics_queue,
                &[submit_info],
                self.fence,
            )
        }
        .map_err(|source| VulkanError::operation("submit BLP texture transfer", source))?;
        // SAFETY: The fence belongs to this exact submitted work.
        unsafe {
            self.context
                .device
                .wait_for_fences(&[self.fence], true, u64::MAX)
        }
        .map_err(|source| VulkanError::operation("wait for BLP texture transfer", source))
    }
}

impl Drop for TextureTransfer<'_> {
    /// Releases submission objects before their staging allocation.
    fn drop(&mut self) {
        // SAFETY: Non-null handles are uniquely owned by this transfer.
        unsafe {
            if self.fence != vk::Fence::null() {
                self.context.device.destroy_fence(self.fence, None);
            }
            if self.command_pool != vk::CommandPool::null() {
                self.context
                    .device
                    .destroy_command_pool(self.command_pool, None);
            }
            if let Some(mut allocation) = self.staging_allocation.take() {
                self.context
                    .allocator
                    .destroy_buffer(self.staging_buffer, &mut allocation);
            }
        }
    }
}

/// Decodes and uploads every authored BLP mip into one immutable sampled image.
pub(super) fn upload_texture(
    context: TextureUploadContext<'_>,
    source: &BlpTextureSource,
    color_space: BlpColorSpace,
) -> Result<GpuBlpTexture, BlpTextureUploadError> {
    let (bytes, mips) = decode_mips(source)?;
    let format = match color_space {
        BlpColorSpace::Linear => vk::Format::R8G8B8A8_UNORM,
        BlpColorSpace::Srgb => vk::Format::R8G8B8A8_SRGB,
    };
    let image = upload_sampled_image(
        context,
        format,
        (source.width(), source.height()),
        &bytes,
        &mips,
    )?;
    let info = BlpTextureResourceInfo::new(
        source.path().clone(),
        BlpTextureSourceKind::Authored,
        color_space,
        (source.width(), source.height()),
        mips.len(),
        bytes.len(),
    );
    Ok(GpuBlpTexture { image, info })
}

/// Uploads stock's opaque 8x8 green WMO placeholder as an sRGB image.
pub(super) fn upload_stock_world_model_green(
    context: TextureUploadContext<'_>,
) -> Result<GpuBlpTexture, BlpTextureUploadError> {
    const EXTENT: (u32, u32) = (8, 8);
    const PIXEL: [u8; 4] = [0, 255, 0, 255];
    const BYTE_COUNT: usize = EXTENT.0 as usize * EXTENT.1 as usize * PIXEL.len();

    let mut bytes = [0_u8; BYTE_COUNT];
    for pixel in bytes.as_chunks_mut::<4>().0 {
        pixel.copy_from_slice(&PIXEL);
    }
    let image = upload_rgba8_image_with_color_space(context, EXTENT, &bytes, BlpColorSpace::Srgb)?;
    let path = solarity_asset::AssetPath::new("SOLARITY\\STOCK\\WMO_GREEN.BLP")?;
    let info = BlpTextureResourceInfo::new(
        path,
        BlpTextureSourceKind::StockWorldModelGreen,
        BlpColorSpace::Srgb,
        EXTENT,
        1,
        bytes.len(),
    );
    Ok(GpuBlpTexture { image, info })
}

/// Uploads one tightly packed linear RGBA8 image for a non-BLP typed owner.
pub(in crate::device) fn upload_rgba8_image(
    context: TextureUploadContext<'_>,
    extent: (u32, u32),
    bytes: &[u8],
) -> Result<GpuSampledImage, VulkanError> {
    upload_rgba8_image_with_color_space(context, extent, bytes, BlpColorSpace::Linear)
}

fn upload_rgba8_image_with_color_space(
    context: TextureUploadContext<'_>,
    extent: (u32, u32),
    bytes: &[u8],
    color_space: BlpColorSpace,
) -> Result<GpuSampledImage, VulkanError> {
    let expected = u64::from(extent.0)
        .checked_mul(u64::from(extent.1))
        .and_then(|pixels| pixels.checked_mul(4))
        .and_then(|byte_count| usize::try_from(byte_count).ok())
        .ok_or_else(|| VulkanError::operation("size RGBA8 image", "extent overflow"))?;
    if extent.0 == 0 || extent.1 == 0 || bytes.len() != expected {
        return Err(VulkanError::operation(
            "validate RGBA8 image",
            format!(
                "{}x{} requires {expected} bytes; received {}",
                extent.0,
                extent.1,
                bytes.len()
            ),
        ));
    }
    let mips = [DecodedMip {
        offset: 0,
        width: extent.0,
        height: extent.1,
    }];
    let format = match color_space {
        BlpColorSpace::Linear => vk::Format::R8G8B8A8_UNORM,
        BlpColorSpace::Srgb => vk::Format::R8G8B8A8_SRGB,
    };
    upload_sampled_image(context, format, extent, bytes, &mips)
}

fn upload_sampled_image(
    context: TextureUploadContext<'_>,
    format: vk::Format,
    extent: (u32, u32),
    bytes: &[u8],
    mips: &[DecodedMip],
) -> Result<GpuSampledImage, VulkanError> {
    let mip_levels = u32::try_from(mips.len())
        .map_err(|source| VulkanError::operation("convert sampled image mip count", source))?;
    if mip_levels == 0 || bytes.is_empty() {
        return Err(VulkanError::operation(
            "validate sampled image",
            "image has no mip pixels",
        ));
    }
    let image_info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(format)
        .extent(vk::Extent3D {
            width: extent.0,
            height: extent.1,
            depth: 1,
        })
        .mip_levels(mip_levels)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED);
    let allocation_info = vk_mem::AllocationCreateInfo {
        usage: vk_mem::MemoryUsage::AutoPreferDevice,
        required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
        ..Default::default()
    };
    // SAFETY: VMA receives a complete image description and owns the binding.
    let (image, allocation) = unsafe {
        context
            .allocator
            .create_image(&image_info, &allocation_info)
    }
    .map_err(|source| VulkanError::operation("create sampled image", source))?;
    let mut guard = TextureGuard {
        device: context.device,
        allocator: context.allocator,
        texture: Some(GpuSampledImage {
            image,
            allocation: Some(allocation),
            view: vk::ImageView::null(),
        }),
    };
    let transfer = TextureTransfer::create(context, bytes)?;
    let command_buffer = transfer.command_buffer()?;
    record_upload(
        context.device,
        command_buffer,
        transfer.staging_buffer,
        image,
        mip_levels,
        mips,
    )?;
    transfer.submit_and_wait(command_buffer)?;
    let view_info = vk::ImageViewCreateInfo::default()
        .image(image)
        .view_type(vk::ImageViewType::TYPE_2D)
        .format(format)
        .subresource_range(
            vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .base_mip_level(0)
                .level_count(mip_levels)
                .base_array_layer(0)
                .layer_count(1),
        );
    // SAFETY: The image is live and the view covers its exact format/mip range.
    let view = unsafe { context.device.create_image_view(&view_info, None) }
        .map_err(|source| VulkanError::operation("create BLP image view", source))?;
    let sampled_image = guard.texture.as_mut().ok_or_else(|| {
        VulkanError::operation("retain sampled image view", "image is unavailable")
    })?;
    sampled_image.view = view;
    guard.finish()
}

/// Expands authored compressed mips only for the duration of GPU staging.
fn decode_mips(source: &BlpTextureSource) -> Result<(Vec<u8>, Vec<DecodedMip>), AssetError> {
    let mut bytes = Vec::new();
    let mut mips = Vec::with_capacity(source.mip_count());
    for mip_level in 0..source.mip_count() {
        let decoded = source.decode_mip(mip_level)?;
        let expected =
            source
                .mip_dimensions(mip_level)
                .ok_or_else(|| AssetError::TextureDecode {
                    path: source.path().clone(),
                    message: format!("authored mip {mip_level} has no dimensions"),
                })?;
        if (decoded.width(), decoded.height()) != expected {
            return Err(AssetError::TextureDecode {
                path: source.path().clone(),
                message: format!(
                    "authored mip {mip_level} decoded as {}x{}, expected {}x{}",
                    decoded.width(),
                    decoded.height(),
                    expected.0,
                    expected.1
                ),
            });
        }
        let offset = u64::try_from(bytes.len()).map_err(|error| AssetError::TextureDecode {
            path: source.path().clone(),
            message: format!("mip staging offset is not addressable: {error}"),
        })?;
        mips.push(DecodedMip {
            offset,
            width: decoded.width(),
            height: decoded.height(),
        });
        bytes.extend_from_slice(decoded.rgba8());
    }
    if bytes.is_empty() {
        return Err(AssetError::TextureDecode {
            path: source.path().clone(),
            message: "BLP has no authored mip pixels".to_owned(),
        });
    }
    Ok((bytes, mips))
}

/// Transitions, copies all mip regions, and exposes them to fragment sampling.
fn record_upload(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    staging_buffer: vk::Buffer,
    image: vk::Image,
    mip_levels: u32,
    mips: &[DecodedMip],
) -> Result<(), VulkanError> {
    let begin =
        vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
    // SAFETY: The primary command buffer is newly allocated and not pending.
    unsafe { device.begin_command_buffer(command_buffer, &begin) }
        .map_err(|source| VulkanError::operation("begin BLP transfer commands", source))?;
    let range = vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(mip_levels)
        .base_array_layer(0)
        .layer_count(1);
    let to_transfer = vk::ImageMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::NONE)
        .src_access_mask(vk::AccessFlags2::NONE)
        .dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
        .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .image(image)
        .subresource_range(range);
    let to_transfer_barriers = [to_transfer];
    let dependency = vk::DependencyInfo::default().image_memory_barriers(&to_transfer_barriers);
    // SAFETY: Synchronization2 is enabled and the image is newly allocated.
    unsafe { device.cmd_pipeline_barrier2(command_buffer, &dependency) };

    let regions = mips
        .iter()
        .enumerate()
        .map(|(level, mip)| {
            Ok(vk::BufferImageCopy::default()
                .buffer_offset(mip.offset)
                .buffer_row_length(0)
                .buffer_image_height(0)
                .image_subresource(
                    vk::ImageSubresourceLayers::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .mip_level(u32::try_from(level).map_err(|source| {
                            VulkanError::operation("convert BLP mip level", source)
                        })?)
                        .base_array_layer(0)
                        .layer_count(1),
                )
                .image_extent(vk::Extent3D {
                    width: mip.width,
                    height: mip.height,
                    depth: 1,
                }))
        })
        .collect::<Result<Vec<_>, VulkanError>>()?;
    // SAFETY: Each tightly packed region addresses its exact decoded mip range.
    unsafe {
        device.cmd_copy_buffer_to_image(
            command_buffer,
            staging_buffer,
            image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &regions,
        );
    }
    let to_sample = vk::ImageMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
        .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
        .dst_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ)
        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        .image(image)
        .subresource_range(range);
    let to_sample_barriers = [to_sample];
    let dependency = vk::DependencyInfo::default().image_memory_barriers(&to_sample_barriers);
    // SAFETY: The transfer writes precede every future fragment sample.
    unsafe { device.cmd_pipeline_barrier2(command_buffer, &dependency) };
    // SAFETY: All referenced resources remain live through submission retirement.
    unsafe { device.end_command_buffer(command_buffer) }
        .map_err(|source| VulkanError::operation("end BLP transfer commands", source))
}
