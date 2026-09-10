//! Authored-mip decoding and one-shot transfer to a sampled Vulkan image.

#![allow(unsafe_code)]

use ash::{Device, vk};
use solarity_asset::{AssetError, BlpBlockCompression, BlpTextureSource};
use vk_mem::Alloc;

use crate::device::VulkanError;

use super::status::BlpTextureUploadError;
use super::types::{
    BlpColorSpace, BlpTextureResourceInfo, BlpTextureSourceKind, BlpTextureStorage,
};

/// Borrowed renderer objects required to submit a texture transfer.
#[derive(Clone, Copy)]
pub(in crate::device) struct TextureUploadContext<'a> {
    pub(in crate::device) device: &'a Device,
    pub(in crate::device) allocator: &'a vk_mem::Allocator,
    pub(in crate::device) graphics_queue: vk::Queue,
    pub(in crate::device) graphics_queue_family: u32,
}

/// One upload mip's byte offset and exact two-dimensional extent.
struct UploadMip {
    offset: vk::DeviceSize,
    width: u32,
    height: u32,
}

/// One tightly packed RGBA8 mip borrowed from a non-BLP typed owner.
#[derive(Clone, Copy)]
pub(in crate::device) struct Rgba8MipUpload<'a> {
    width: u32,
    height: u32,
    bytes: &'a [u8],
}

impl<'a> Rgba8MipUpload<'a> {
    /// Couples one exact two-dimensional mip extent to its RGBA8 pixels.
    pub(in crate::device) const fn new(width: u32, height: u32, bytes: &'a [u8]) -> Self {
        Self {
            width,
            height,
            bytes,
        }
    }

    /// Returns this mip's width.
    pub(in crate::device) const fn width(self) -> u32 {
        self.width
    }

    /// Returns this mip's height.
    pub(in crate::device) const fn height(self) -> u32 {
        self.height
    }
}

/// One staging payload retaining either authored BC blocks or decoded RGBA8.
struct PreparedTexture {
    bytes: Vec<u8>,
    mips: Vec<UploadMip>,
    storage: BlpTextureStorage,
}

/// One allocated image awaiting the shared transfer and view creation.
struct PendingBatchTexture {
    image: GpuSampledImage,
    format: vk::Format,
    mip_levels: u32,
    mips: Vec<UploadMip>,
    info: BlpTextureResourceInfo,
}

/// Borrowed recording description for one destination image.
struct ImageUpload<'mips> {
    image: vk::Image,
    mip_levels: u32,
    mips: &'mips [UploadMip],
}

/// Owned staging representation of a validated RGBA8 mip chain.
struct PackedRgba8Mips {
    extent: (u32, u32),
    bytes: Vec<u8>,
    mips: Vec<UploadMip>,
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

/// Guard ensuring a partially admitted batch releases every permanent image.
struct TextureBatchGuard<'a> {
    device: &'a Device,
    allocator: &'a vk_mem::Allocator,
    textures: Vec<PendingBatchTexture>,
}

impl TextureBatchGuard<'_> {
    /// Transfers fully initialized images into registry-owned resources.
    fn finish(mut self) -> Vec<GpuBlpTexture> {
        std::mem::take(&mut self.textures)
            .into_iter()
            .map(|pending| GpuBlpTexture {
                image: pending.image,
                info: pending.info,
            })
            .collect()
    }
}

impl Drop for TextureBatchGuard<'_> {
    fn drop(&mut self) {
        for pending in self.textures.iter_mut().rev() {
            pending.image.destroy(self.device, self.allocator);
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

/// Submitted staging resources retained until their transfer fence signals.
///
/// A later graphics submission on the same queue can sample the destination
/// without a host wait: queue order plus the recorded image barriers establish
/// the dependency. Only staging destruction must be deferred.
pub(in crate::device) struct DeferredTextureTransfer {
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
        self.submit(command_buffer)?;
        // SAFETY: The fence belongs to this exact submitted work.
        unsafe {
            self.context
                .device
                .wait_for_fences(&[self.fence], true, u64::MAX)
        }
        .map_err(|source| VulkanError::operation("wait for BLP texture transfer", source))
    }

    /// Queues the transfer without stalling the host presentation thread.
    fn submit(&self, command_buffer: vk::CommandBuffer) -> Result<(), VulkanError> {
        let command_info = vk::CommandBufferSubmitInfo::default().command_buffer(command_buffer);
        let command_infos = [command_info];
        let submit_info = vk::SubmitInfo2::default().command_buffer_infos(&command_infos);
        // SAFETY: The executable command and every resource it references are
        // retained either through the immediate wait or a deferred owner.
        unsafe {
            self.context.device.queue_submit2(
                self.context.graphics_queue,
                &[submit_info],
                self.fence,
            )
        }
        .map_err(|source| VulkanError::operation("submit BLP texture transfer", source))
    }

    /// Transfers staging lifetime to a renderer registry after submission.
    fn defer(mut self) -> DeferredTextureTransfer {
        DeferredTextureTransfer {
            staging_buffer: std::mem::replace(&mut self.staging_buffer, vk::Buffer::null()),
            staging_allocation: self.staging_allocation.take(),
            command_pool: std::mem::replace(&mut self.command_pool, vk::CommandPool::null()),
            fence: std::mem::replace(&mut self.fence, vk::Fence::null()),
        }
    }
}

impl DeferredTextureTransfer {
    /// Reports whether the GPU has retired this staging generation.
    pub(in crate::device) fn is_complete(&self, device: &Device) -> Result<bool, VulkanError> {
        // SAFETY: The fence remains owned by this pending transfer.
        unsafe { device.get_fence_status(self.fence) }
            .map_err(|source| VulkanError::operation("poll deferred texture transfer", source))
    }

    /// Releases a completed transfer's command and staging resources.
    pub(in crate::device) fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        // SAFETY: Callers retire only signaled transfers or invoke this after
        // renderer-wide device idle during shutdown.
        unsafe {
            if self.fence != vk::Fence::null() {
                device.destroy_fence(self.fence, None);
                self.fence = vk::Fence::null();
            }
            if self.command_pool != vk::CommandPool::null() {
                device.destroy_command_pool(self.command_pool, None);
                self.command_pool = vk::CommandPool::null();
            }
            if let Some(mut allocation) = self.staging_allocation.take() {
                allocator.destroy_buffer(self.staging_buffer, &mut allocation);
                self.staging_buffer = vk::Buffer::null();
            }
        }
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

/// Queues all requested sources through one staging allocation and submission.
///
/// Image views are complete before submission, allowing the returned resources
/// to enter descriptor preparation immediately. Queue order establishes the
/// transfer-to-sample dependency; the returned owner retains staging storage
/// until its fence signals.
pub(super) fn upload_textures_deferred(
    context: TextureUploadContext<'_>,
    requests: &[(&BlpTextureSource, BlpColorSpace)],
) -> Result<(Vec<GpuBlpTexture>, Option<DeferredTextureTransfer>), BlpTextureUploadError> {
    if requests.is_empty() {
        return Ok((Vec::new(), None));
    }

    let mut staging_byte_count = 0_usize;
    for (source, _color_space) in requests {
        staging_byte_count = align_up(
            staging_byte_count,
            texel_block_byte_count(source_storage(source)),
        )
        .ok_or_else(|| AssetError::TextureDecode {
            path: source.path().clone(),
            message: "BLP batch staging alignment overflows".to_owned(),
        })?;
        staging_byte_count = staging_byte_count
            .checked_add(prepared_byte_count(source)?)
            .ok_or_else(|| AssetError::TextureDecode {
                path: source.path().clone(),
                message: "BLP batch staging byte count overflows".to_owned(),
            })?;
    }

    let mut staging_bytes = Vec::with_capacity(staging_byte_count);
    let mut prepared_textures = Vec::with_capacity(requests.len());
    for (source, color_space) in requests {
        let mut prepared = prepare_mips(source)?;
        let aligned_offset = align_up(
            staging_bytes.len(),
            texel_block_byte_count(prepared.storage),
        )
        .ok_or_else(|| AssetError::TextureDecode {
            path: source.path().clone(),
            message: "BLP batch staging alignment overflows".to_owned(),
        })?;
        staging_bytes.resize(aligned_offset, 0);
        let base_offset =
            u64::try_from(staging_bytes.len()).map_err(|error| AssetError::TextureDecode {
                path: source.path().clone(),
                message: format!("BLP batch staging offset is not addressable: {error}"),
            })?;
        for mip in &mut prepared.mips {
            mip.offset =
                mip.offset
                    .checked_add(base_offset)
                    .ok_or_else(|| AssetError::TextureDecode {
                        path: source.path().clone(),
                        message: "BLP batch mip offset overflows".to_owned(),
                    })?;
        }
        staging_bytes.extend_from_slice(&prepared.bytes);
        let format = texture_format(prepared.storage, *color_space);
        let mip_levels = u32::try_from(prepared.mips.len())
            .map_err(|error| VulkanError::operation("convert sampled image mip count", error))?;
        let info = BlpTextureResourceInfo::new(
            source.path().clone(),
            BlpTextureSourceKind::Authored,
            *color_space,
            prepared.storage,
            (source.width(), source.height()),
            prepared.mips.len(),
            prepared.bytes.len(),
        );
        prepared_textures.push((
            format,
            mip_levels,
            (source.width(), source.height()),
            prepared.mips,
            info,
        ));
    }
    if staging_bytes.len() != staging_byte_count {
        let source = requests[0].0;
        return Err(AssetError::TextureDecode {
            path: source.path().clone(),
            message: format!(
                "BLP batch contains {} upload bytes; expected {staging_byte_count}",
                staging_bytes.len()
            ),
        }
        .into());
    }

    let mut guard = TextureBatchGuard {
        device: context.device,
        allocator: context.allocator,
        textures: Vec::with_capacity(prepared_textures.len()),
    };
    for (format, mip_levels, extent, mips, info) in prepared_textures {
        let image = allocate_sampled_image(context, format, extent, mip_levels)?;
        guard.textures.push(PendingBatchTexture {
            image,
            format,
            mip_levels,
            mips,
            info,
        });
    }

    let transfer = TextureTransfer::create(context, &staging_bytes)?;
    let command_buffer = transfer.command_buffer()?;
    let uploads = guard
        .textures
        .iter()
        .map(|pending| ImageUpload {
            image: pending.image.image,
            mip_levels: pending.mip_levels,
            mips: &pending.mips,
        })
        .collect::<Vec<_>>();
    record_uploads(
        context.device,
        command_buffer,
        transfer.staging_buffer,
        &uploads,
    )?;
    // Complete every fallible host operation before queue submission. Once a
    // transfer is executing, an error path must not destroy its destinations.
    for pending in &mut guard.textures {
        pending.image.view = create_sampled_image_view(
            context.device,
            pending.image.image,
            pending.format,
            pending.mip_levels,
        )?;
    }
    transfer.submit(command_buffer)?;
    Ok((guard.finish(), Some(transfer.defer())))
}

/// Uploads stock's opaque 8x8 green WMO placeholder without transfer conversion.
pub(super) fn upload_stock_world_model_green(
    context: TextureUploadContext<'_>,
) -> Result<GpuBlpTexture, BlpTextureUploadError> {
    upload_stock_solid_texture(
        context,
        [0, 255, 0, 255],
        "SOLARITY\\STOCK\\WMO_GREEN.BLP",
        BlpTextureSourceKind::StockWorldModelGreen,
        BlpColorSpace::Linear,
    )
}

/// Uploads stock's opaque 8x8 white image for an empty M2 filename.
pub(super) fn upload_stock_m2_white(
    context: TextureUploadContext<'_>,
) -> Result<GpuBlpTexture, BlpTextureUploadError> {
    upload_stock_solid_texture(
        context,
        [255, 255, 255, 255],
        "SOLARITY\\STOCK\\M2_WHITE.BLP",
        BlpTextureSourceKind::StockM2White,
        BlpColorSpace::Linear,
    )
}

/// Uploads stock's opaque 8x8 green image for a failed M2 texture request.
pub(super) fn upload_stock_m2_failure(
    context: TextureUploadContext<'_>,
) -> Result<GpuBlpTexture, BlpTextureUploadError> {
    upload_stock_solid_texture(
        context,
        [0, 255, 0, 255],
        "SOLARITY\\STOCK\\M2_FAILURE.BLP",
        BlpTextureSourceKind::StockM2Failure,
        BlpColorSpace::Linear,
    )
}

/// Materializes one of Texture.cpp's renderer-local generated images.
fn upload_stock_solid_texture(
    context: TextureUploadContext<'_>,
    pixel: [u8; 4],
    identity: &str,
    source_kind: BlpTextureSourceKind,
    color_space: BlpColorSpace,
) -> Result<GpuBlpTexture, BlpTextureUploadError> {
    const EXTENT: (u32, u32) = (8, 8);
    const BYTE_COUNT: usize = EXTENT.0 as usize * EXTENT.1 as usize * 4;

    let mut bytes = [0_u8; BYTE_COUNT];
    for texel in bytes.as_chunks_mut::<4>().0 {
        texel.copy_from_slice(&pixel);
    }
    let image = upload_rgba8_image_with_color_space(context, EXTENT, &bytes, color_space)?;
    let path = solarity_asset::AssetPath::new(identity)?;
    let info = BlpTextureResourceInfo::new(
        path,
        source_kind,
        color_space,
        BlpTextureStorage::Rgba8,
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

/// Queues a linear RGBA8 image while retaining its borrowed pixels only for staging.
///
/// Later draws on the same graphics queue observe the recorded image barriers.
/// The caller retains the returned staging owner until its transfer completes.
pub(in crate::device) fn upload_rgba8_image_deferred(
    context: TextureUploadContext<'_>,
    extent: (u32, u32),
    bytes: &[u8],
) -> Result<(GpuSampledImage, DeferredTextureTransfer), VulkanError> {
    validate_rgba8_image(extent, bytes)?;
    let mips = [UploadMip {
        offset: 0,
        width: extent.0,
        height: extent.1,
    }];
    let format = texture_format(BlpTextureStorage::Rgba8, BlpColorSpace::Linear);
    upload_sampled_image_deferred(context, format, extent, bytes, &mips)
}

/// Queues a generated two-channel signed displacement image on the graphics queue.
pub(in crate::device) fn upload_rg8_snorm_image_deferred(
    context: TextureUploadContext<'_>,
    extent: (u32, u32),
    bytes: &[u8],
) -> Result<(GpuSampledImage, DeferredTextureTransfer), VulkanError> {
    let expected = (u64::from(extent.0) * u64::from(extent.1)).checked_mul(2);
    if extent.0 == 0
        || extent.1 == 0
        || expected.is_none()
        || u64::try_from(bytes.len()).ok() != expected
    {
        return Err(VulkanError::operation(
            "validate RG8 signed image",
            "extent and displacement bytes differ",
        ));
    }
    let mips = [UploadMip {
        offset: 0,
        width: extent.0,
        height: extent.1,
    }];
    upload_sampled_image_deferred(context, vk::Format::R8G8_SNORM, extent, bytes, &mips)
}

/// Queues one complete RGBA8 mip chain and returns its staging retirement owner.
///
/// The image may enter descriptor and draw preparation immediately when those
/// draws are submitted to the same graphics queue after this transfer.
pub(in crate::device) fn upload_rgba8_mip_chain_deferred(
    context: TextureUploadContext<'_>,
    source_mips: &[Rgba8MipUpload<'_>],
    color_space: BlpColorSpace,
) -> Result<(GpuSampledImage, DeferredTextureTransfer), VulkanError> {
    let packed = pack_rgba8_mips(source_mips)?;
    let format = texture_format(BlpTextureStorage::Rgba8, color_space);
    upload_sampled_image_deferred(context, format, packed.extent, &packed.bytes, &packed.mips)
}

fn pack_rgba8_mips(source_mips: &[Rgba8MipUpload<'_>]) -> Result<PackedRgba8Mips, VulkanError> {
    let top = source_mips.first().ok_or_else(|| {
        VulkanError::operation("validate RGBA8 mip chain", "image has no mip pixels")
    })?;
    let mut bytes = Vec::new();
    let mut mips = Vec::with_capacity(source_mips.len());
    for (level, source) in source_mips.iter().copied().enumerate() {
        let expected = u64::from(source.width)
            .checked_mul(u64::from(source.height))
            .and_then(|pixels| pixels.checked_mul(4))
            .and_then(|byte_count| usize::try_from(byte_count).ok())
            .ok_or_else(|| VulkanError::operation("size RGBA8 mip", "extent overflow"))?;
        if source.width == 0 || source.height == 0 || source.bytes.len() != expected {
            return Err(VulkanError::operation(
                "validate RGBA8 mip chain",
                format!(
                    "mip {level} {}x{} requires {expected} bytes; received {}",
                    source.width,
                    source.height,
                    source.bytes.len()
                ),
            ));
        }
        let offset = u64::try_from(bytes.len())
            .map_err(|source| VulkanError::operation("convert RGBA8 mip offset", source))?;
        mips.push(UploadMip {
            offset,
            width: source.width,
            height: source.height,
        });
        bytes.extend_from_slice(source.bytes);
    }
    Ok(PackedRgba8Mips {
        extent: (top.width, top.height),
        bytes,
        mips,
    })
}

fn upload_rgba8_image_with_color_space(
    context: TextureUploadContext<'_>,
    extent: (u32, u32),
    bytes: &[u8],
    color_space: BlpColorSpace,
) -> Result<GpuSampledImage, VulkanError> {
    validate_rgba8_image(extent, bytes)?;
    let mips = [UploadMip {
        offset: 0,
        width: extent.0,
        height: extent.1,
    }];
    let format = texture_format(BlpTextureStorage::Rgba8, color_space);
    upload_sampled_image(context, format, extent, bytes, &mips)
}

/// Rejects incomplete pixels before any synchronous or deferred allocation.
fn validate_rgba8_image(extent: (u32, u32), bytes: &[u8]) -> Result<(), VulkanError> {
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
    Ok(())
}

fn upload_sampled_image(
    context: TextureUploadContext<'_>,
    format: vk::Format,
    extent: (u32, u32),
    bytes: &[u8],
    mips: &[UploadMip],
) -> Result<GpuSampledImage, VulkanError> {
    let mip_levels = u32::try_from(mips.len())
        .map_err(|source| VulkanError::operation("convert sampled image mip count", source))?;
    if mip_levels == 0 || bytes.is_empty() {
        return Err(VulkanError::operation(
            "validate sampled image",
            "image has no mip pixels",
        ));
    }
    let image = allocate_sampled_image(context, format, extent, mip_levels)?;
    let mut guard = TextureGuard {
        device: context.device,
        allocator: context.allocator,
        texture: Some(image),
    };
    let transfer = TextureTransfer::create(context, bytes)?;
    let command_buffer = transfer.command_buffer()?;
    let image_handle = guard
        .texture
        .as_ref()
        .ok_or_else(|| {
            VulkanError::operation("record sampled image upload", "image is unavailable")
        })?
        .image;
    let upload = ImageUpload {
        image: image_handle,
        mip_levels,
        mips,
    };
    record_uploads(
        context.device,
        command_buffer,
        transfer.staging_buffer,
        &[upload],
    )?;
    transfer.submit_and_wait(command_buffer)?;
    let view = create_sampled_image_view(context.device, image_handle, format, mip_levels)?;
    let sampled_image = guard.texture.as_mut().ok_or_else(|| {
        VulkanError::operation("retain sampled image view", "image is unavailable")
    })?;
    sampled_image.view = view;
    guard.finish()
}

fn upload_sampled_image_deferred(
    context: TextureUploadContext<'_>,
    format: vk::Format,
    extent: (u32, u32),
    bytes: &[u8],
    mips: &[UploadMip],
) -> Result<(GpuSampledImage, DeferredTextureTransfer), VulkanError> {
    let mip_levels = u32::try_from(mips.len())
        .map_err(|source| VulkanError::operation("convert sampled image mip count", source))?;
    if mip_levels == 0 || bytes.is_empty() {
        return Err(VulkanError::operation(
            "validate sampled image",
            "image has no mip pixels",
        ));
    }
    let image = allocate_sampled_image(context, format, extent, mip_levels)?;
    let mut guard = TextureGuard {
        device: context.device,
        allocator: context.allocator,
        texture: Some(image),
    };
    let transfer = TextureTransfer::create(context, bytes)?;
    let command_buffer = transfer.command_buffer()?;
    let image_handle = guard
        .texture
        .as_ref()
        .ok_or_else(|| {
            VulkanError::operation("record sampled image upload", "image is unavailable")
        })?
        .image;
    record_uploads(
        context.device,
        command_buffer,
        transfer.staging_buffer,
        &[ImageUpload {
            image: image_handle,
            mip_levels,
            mips,
        }],
    )?;
    // Complete all fallible host setup before submission. An error must never
    // destroy an image whose transfer is already executing.
    let view = create_sampled_image_view(context.device, image_handle, format, mip_levels)?;
    let sampled_image = guard.texture.as_mut().ok_or_else(|| {
        VulkanError::operation("retain sampled image view", "image is unavailable")
    })?;
    sampled_image.view = view;
    transfer.submit(command_buffer)?;
    Ok((guard.finish()?, transfer.defer()))
}

/// Allocates one device-local sampled transfer destination without a view.
fn allocate_sampled_image(
    context: TextureUploadContext<'_>,
    format: vk::Format,
    extent: (u32, u32),
    mip_levels: u32,
) -> Result<GpuSampledImage, VulkanError> {
    if extent.0 == 0 || extent.1 == 0 || mip_levels == 0 {
        return Err(VulkanError::operation(
            "validate sampled image allocation",
            "image extent and mip count must be nonzero",
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
    Ok(GpuSampledImage {
        image,
        allocation: Some(allocation),
        view: vk::ImageView::null(),
    })
}

/// Creates the full color-mip view after transfer completion.
fn create_sampled_image_view(
    device: &Device,
    image: vk::Image,
    format: vk::Format,
    mip_levels: u32,
) -> Result<vk::ImageView, VulkanError> {
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
    unsafe { device.create_image_view(&view_info, None) }
        .map_err(|source| VulkanError::operation("create BLP image view", source))
}

/// Preflights one exact upload footprint without expanding source pixels.
fn prepared_byte_count(source: &BlpTextureSource) -> Result<usize, AssetError> {
    if source.block_compression().is_none() {
        return source.decoded_rgba8_byte_count();
    }

    let mut byte_count = 0_usize;
    for mip_level in 0..source.mip_count() {
        let mip = source
            .block_mip(mip_level)
            .ok_or_else(|| AssetError::TextureDecode {
                path: source.path().clone(),
                message: format!("authored BC mip {mip_level} is unavailable"),
            })?;
        byte_count = byte_count
            .checked_add(mip.upload_byte_count())
            .ok_or_else(|| AssetError::TextureDecode {
                path: source.path().clone(),
                message: "authored BC mip-chain byte count overflows".to_owned(),
            })?;
    }
    Ok(byte_count)
}

/// Returns the destination format's Vulkan texel-block size.
const fn texel_block_byte_count(storage: BlpTextureStorage) -> usize {
    match storage {
        BlpTextureStorage::Rgba8 => 4,
        BlpTextureStorage::Bc1 => 8,
        BlpTextureStorage::Bc2 | BlpTextureStorage::Bc3 => 16,
    }
}

/// Determines destination storage without decoding any authored mip.
fn source_storage(source: &BlpTextureSource) -> BlpTextureStorage {
    match source.block_compression() {
        Some(BlpBlockCompression::Bc1) => BlpTextureStorage::Bc1,
        Some(BlpBlockCompression::Bc2) => BlpTextureStorage::Bc2,
        Some(BlpBlockCompression::Bc3) => BlpTextureStorage::Bc3,
        None => BlpTextureStorage::Rgba8,
    }
}

/// Rounds one byte offset to a power-of-two texel-block boundary.
const fn align_up(value: usize, alignment: usize) -> Option<usize> {
    let mask = alignment - 1;
    match value.checked_add(mask) {
        Some(sum) => Some(sum & !mask),
        None => None,
    }
}

/// Preserves authored BC blocks and decodes only encodings Vulkan cannot sample.
fn prepare_mips(source: &BlpTextureSource) -> Result<PreparedTexture, AssetError> {
    if let Some(compression) = source.block_compression() {
        return prepare_block_mips(source, compression);
    }

    let decoded_byte_count = source.decoded_rgba8_byte_count()?;
    let mut bytes = Vec::with_capacity(decoded_byte_count);
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
        mips.push(UploadMip {
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
    if bytes.len() != decoded_byte_count {
        return Err(AssetError::TextureDecode {
            path: source.path().clone(),
            message: format!(
                "decoded mip chain contains {} bytes; expected {decoded_byte_count}",
                bytes.len()
            ),
        });
    }
    Ok(PreparedTexture {
        bytes,
        mips,
        storage: BlpTextureStorage::Rgba8,
    })
}

/// Packs authored DXT blocks into one block-aligned transfer payload.
fn prepare_block_mips(
    source: &BlpTextureSource,
    compression: BlpBlockCompression,
) -> Result<PreparedTexture, AssetError> {
    let byte_count = prepared_byte_count(source)?;

    let mut bytes = Vec::with_capacity(byte_count);
    let mut mips = Vec::with_capacity(source.mip_count());
    for mip_level in 0..source.mip_count() {
        let mip = source
            .block_mip(mip_level)
            .ok_or_else(|| AssetError::TextureDecode {
                path: source.path().clone(),
                message: format!("authored BC mip {mip_level} is unavailable"),
            })?;
        let offset = u64::try_from(bytes.len()).map_err(|error| AssetError::TextureDecode {
            path: source.path().clone(),
            message: format!("BC mip staging offset is not addressable: {error}"),
        })?;
        mips.push(UploadMip {
            offset,
            width: mip.width(),
            height: mip.height(),
        });

        let required = mip.upload_byte_count();
        let retained = mip.bytes();
        bytes.extend_from_slice(&retained[..retained.len().min(required)]);
        // Small authored tail mips can contain fewer whole blocks than their
        // logical extent. Zero padding is the existing stock-compatible BLP
        // decoder behavior and produces the exact Vulkan copy footprint.
        bytes.resize(bytes.len() + required.saturating_sub(retained.len()), 0);
    }
    if bytes.is_empty() {
        return Err(AssetError::TextureDecode {
            path: source.path().clone(),
            message: "BLP has no authored BC mip blocks".to_owned(),
        });
    }
    if bytes.len() != byte_count {
        return Err(AssetError::TextureDecode {
            path: source.path().clone(),
            message: format!(
                "BC mip chain contains {} upload bytes; expected {byte_count}",
                bytes.len()
            ),
        });
    }

    let storage = match compression {
        BlpBlockCompression::Bc1 => BlpTextureStorage::Bc1,
        BlpBlockCompression::Bc2 => BlpTextureStorage::Bc2,
        BlpBlockCompression::Bc3 => BlpTextureStorage::Bc3,
    };
    Ok(PreparedTexture {
        bytes,
        mips,
        storage,
    })
}

/// Maps authored storage and caller-owned color interpretation to Vulkan.
const fn texture_format(storage: BlpTextureStorage, color_space: BlpColorSpace) -> vk::Format {
    match (storage, color_space) {
        (BlpTextureStorage::Rgba8, BlpColorSpace::Linear) => vk::Format::R8G8B8A8_UNORM,
        (BlpTextureStorage::Rgba8, BlpColorSpace::Srgb) => vk::Format::R8G8B8A8_SRGB,
        (BlpTextureStorage::Bc1, BlpColorSpace::Linear) => vk::Format::BC1_RGBA_UNORM_BLOCK,
        (BlpTextureStorage::Bc1, BlpColorSpace::Srgb) => vk::Format::BC1_RGBA_SRGB_BLOCK,
        (BlpTextureStorage::Bc2, BlpColorSpace::Linear) => vk::Format::BC2_UNORM_BLOCK,
        (BlpTextureStorage::Bc2, BlpColorSpace::Srgb) => vk::Format::BC2_SRGB_BLOCK,
        (BlpTextureStorage::Bc3, BlpColorSpace::Linear) => vk::Format::BC3_UNORM_BLOCK,
        (BlpTextureStorage::Bc3, BlpColorSpace::Srgb) => vk::Format::BC3_SRGB_BLOCK,
    }
}

/// Transitions and copies every destination in one command buffer.
fn record_uploads(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    staging_buffer: vk::Buffer,
    uploads: &[ImageUpload<'_>],
) -> Result<(), VulkanError> {
    let begin =
        vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
    // SAFETY: The primary command buffer is newly allocated and not pending.
    unsafe { device.begin_command_buffer(command_buffer, &begin) }
        .map_err(|source| VulkanError::operation("begin BLP transfer commands", source))?;
    for upload in uploads {
        let range = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .base_mip_level(0)
            .level_count(upload.mip_levels)
            .base_array_layer(0)
            .layer_count(1);
        let to_transfer = vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::NONE)
            .src_access_mask(vk::AccessFlags2::NONE)
            .dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
            .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .image(upload.image)
            .subresource_range(range);
        let to_transfer_barriers = [to_transfer];
        let dependency = vk::DependencyInfo::default().image_memory_barriers(&to_transfer_barriers);
        // SAFETY: Synchronization2 is enabled and the image is newly allocated.
        unsafe { device.cmd_pipeline_barrier2(command_buffer, &dependency) };

        let regions = upload
            .mips
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
        // SAFETY: Every region addresses its exact nonoverlapping staging span.
        unsafe {
            device.cmd_copy_buffer_to_image(
                command_buffer,
                staging_buffer,
                upload.image,
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
            .image(upload.image)
            .subresource_range(range);
        let to_sample_barriers = [to_sample];
        let dependency = vk::DependencyInfo::default().image_memory_barriers(&to_sample_barriers);
        // SAFETY: The transfer writes precede every future fragment sample.
        unsafe { device.cmd_pipeline_barrier2(command_buffer, &dependency) };
    }
    // SAFETY: All referenced resources remain live through submission retirement.
    unsafe { device.end_command_buffer(command_buffer) }
        .map_err(|source| VulkanError::operation("end BLP transfer commands", source))
}
