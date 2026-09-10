//! Retained 256x128 screen-filter history, independent of presentation slots.

use std::cell::Cell;

use super::*;
use crate::device::vulkan_texture::upload_rgba8_image_deferred;

const EXTENT: vk::Extent2D = vk::Extent2D {
    width: 256,
    height: 128,
};
const FORMAT: vk::Format = vk::Format::R8G8B8A8_UNORM;
const NOISE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/special-noise.rgba"));

pub(super) struct SpecialResources {
    seeded: GlowImage,
    history: GlowImage,
    noise: Option<GpuSampledImage>,
    transfer: Option<DeferredTextureTransfer>,
    pool: vk::DescriptorPool,
    seed: vk::Pipeline,
    propagate: vk::Pipeline,
    combine: vk::Pipeline,
    sets: Vec<vk::DescriptorSet>,
    live: Cell<bool>,
}

impl VulkanGlowRenderer {
    pub(in crate::device) fn ensure_special(
        &mut self,
        context: TextureUploadContext<'_>,
    ) -> Result<(), VulkanError> {
        if let Some(special) = self.special.as_mut() {
            if let Some(transfer) = special.transfer.as_mut()
                && transfer.is_complete(context.device)?
            {
                transfer.destroy(context.device, context.allocator);
                special.transfer = None;
            }
            return Ok(());
        }
        let mut special = SpecialResources {
            seeded: GlowImage::empty(),
            history: GlowImage::empty(),
            noise: None,
            transfer: None,
            pool: vk::DescriptorPool::null(),
            seed: vk::Pipeline::null(),
            propagate: vk::Pipeline::null(),
            combine: vk::Pipeline::null(),
            sets: Vec::new(),
            live: Cell::new(false),
        };
        if let Err(error) = special.create(context, self) {
            special.destroy(context.device, context.allocator);
            return Err(error);
        }
        self.special = Some(special);
        Ok(())
    }
}

impl SpecialResources {
    fn create(
        &mut self,
        context: TextureUploadContext<'_>,
        owner: &VulkanGlowRenderer,
    ) -> Result<(), VulkanError> {
        let device = context.device;
        let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
        self.seeded = GlowImage::create(device, context.allocator, FORMAT, EXTENT, usage)?;
        self.history = GlowImage::create(device, context.allocator, FORMAT, EXTENT, usage)?;
        let compiler = GlowSpirvCompiler::new().map_err(glow_shader_error)?;
        for (pipeline, pass, format) in [
            (&mut self.seed, GlowShaderPass::SpecialSeed, FORMAT),
            (
                &mut self.propagate,
                GlowShaderPass::SpecialPropagate,
                FORMAT,
            ),
            (
                &mut self.combine,
                GlowShaderPass::SpecialCombine,
                owner.format,
            ),
        ] {
            *pipeline = create_pipeline(
                device,
                owner.pipeline_layout,
                format,
                &compiler.compile(pass).map_err(glow_shader_error)?,
            )?;
        }
        let count = u32::try_from(owner.slots.len() + 2).map_err(|_error| {
            VulkanError::operation(
                "create screen-filter descriptors",
                "too many presentation slots",
            )
        })?;
        let sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(count * 3)];
        let info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(count)
            .pool_sizes(&sizes);
        // SAFETY: Counts cover all three bindings in the shared layout.
        self.pool = unsafe { device.create_descriptor_pool(&info, None) }.map_err(|error| {
            VulkanError::operation("create screen-filter descriptor pool", error)
        })?;
        let layouts = vec![owner.set_layout; count as usize];
        let info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(self.pool)
            .set_layouts(&layouts);
        // SAFETY: This pool has capacity for every requested layout.
        self.sets = unsafe { device.allocate_descriptor_sets(&info) }
            .map_err(|error| VulkanError::operation("allocate screen-filter descriptors", error))?;
        // Noise is generated at build time; staging remains owned until the queue completes.
        let (noise, transfer) = upload_rgba8_image_deferred(context, (256, 256), NOISE)?;
        update_set(
            device,
            self.sets[0],
            owner.sampler,
            self.history.view,
            noise.view(),
        );
        update_set(
            device,
            self.sets[1],
            owner.sampler,
            self.seeded.view,
            self.seeded.view,
        );
        for (slot, &set) in owner.slots.iter().zip(&self.sets[2..]) {
            update_set(
                device,
                set,
                owner.sampler,
                self.history.view,
                slot.scene.view,
            );
        }
        self.noise = Some(noise);
        self.transfer = Some(transfer);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn record(
        &self,
        owner: &VulkanGlowRenderer,
        device: &Device,
        command: vk::CommandBuffer,
        destination: vk::ImageView,
        image_index: u32,
        extent: vk::Extent2D,
        window: WorldScreenWindow,
        frame: WorldSpecialFrame,
    ) {
        let live = self.live.replace(true);
        if !live {
            // The clear branch never samples these undefined pixels. Establish a
            // valid descriptor layout before the first seed pass.
            transition_image(
                device,
                command,
                self.history.image,
                vk::PipelineStageFlags2::NONE,
                vk::AccessFlags2::NONE,
                vk::ImageLayout::UNDEFINED,
                vk::PipelineStageFlags2::FRAGMENT_SHADER,
                vk::AccessFlags2::SHADER_SAMPLED_READ,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            );
        }
        let color = [16, 8, 0, 24].map(|shift| ((frame.color >> shift) & 255) as f32 / 255.);
        let mut parameters = [0.; 20];
        parameters[0] = frame.seed_row as f32;
        parameters[1] = f32::from(frame.clear_history || !live);
        parameters[4..8].copy_from_slice(&color);
        self.draw_history(
            owner,
            device,
            command,
            &self.seeded,
            self.seed,
            self.sets[0],
            parameters,
            live,
        );
        parameters[0] = frame.decay;
        self.draw_history(
            owner,
            device,
            command,
            &self.history,
            self.propagate,
            self.sets[1],
            parameters,
            true,
        );
        parameters[..8].copy_from_slice(&[
            frame.desaturation,
            frame.whitening,
            0.,
            0.,
            extent.width as f32,
            extent.height as f32,
            0.,
            0.,
        ]);
        begin_color_target(
            device,
            command,
            destination,
            extent,
            vk::AttachmentLoadOp::LOAD,
        );
        set_viewport_scissor(device, command, extent, Some(window));
        bind_and_draw(
            device,
            command,
            self.combine,
            owner.pipeline_layout,
            self.sets[2 + image_index as usize],
            parameters,
            6144,
        );
        // SAFETY: The composition's rendering scope is active.
        unsafe { device.cmd_end_rendering(command) };
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_history(
        &self,
        owner: &VulkanGlowRenderer,
        device: &Device,
        command: vk::CommandBuffer,
        target: &GlowImage,
        pipeline: vk::Pipeline,
        set: vk::DescriptorSet,
        parameters: [f32; 20],
        live: bool,
    ) {
        // The same graphics queue serializes this history across presentation slots.
        // Include prior reads before overwriting either image on the next frame.
        transition_image(
            device,
            command,
            target.image,
            vk::PipelineStageFlags2::FRAGMENT_SHADER,
            vk::AccessFlags2::SHADER_SAMPLED_READ,
            if live {
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
            } else {
                vk::ImageLayout::UNDEFINED
            },
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        );
        begin_color_target(
            device,
            command,
            target.view,
            EXTENT,
            vk::AttachmentLoadOp::DONT_CARE,
        );
        set_viewport_scissor(device, command, EXTENT, None);
        bind_and_draw(
            device,
            command,
            pipeline,
            owner.pipeline_layout,
            set,
            parameters,
            3,
        );
        // SAFETY: The history target's rendering scope is active.
        unsafe { device.cmd_end_rendering(command) };
        transition_image(
            device,
            command,
            target.image,
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::PipelineStageFlags2::FRAGMENT_SHADER,
            vk::AccessFlags2::SHADER_SAMPLED_READ,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        );
    }

    pub(super) fn destroy(&mut self, device: &Device, allocator: &vk_mem::Allocator) {
        if let Some(mut transfer) = self.transfer.take() {
            transfer.destroy(device, allocator);
        }
        if let Some(mut noise) = self.noise.take() {
            noise.destroy(device, allocator);
        }
        // SAFETY: The parent retires these resources only after the device is idle.
        unsafe {
            for pipeline in [&mut self.seed, &mut self.propagate, &mut self.combine] {
                if *pipeline != vk::Pipeline::null() {
                    device.destroy_pipeline(*pipeline, None);
                }
                *pipeline = vk::Pipeline::null();
            }
            if self.pool != vk::DescriptorPool::null() {
                device.destroy_descriptor_pool(self.pool, None);
            }
        }
        self.pool = vk::DescriptorPool::null();
        self.history.destroy(device, allocator);
        self.seeded.destroy(device, allocator);
        self.sets.clear();
    }
}
