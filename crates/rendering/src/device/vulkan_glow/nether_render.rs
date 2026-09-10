//! Two native distortion draws followed by the invisibility scene composition.

use super::*;

impl VulkanGlowRenderer {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn record_nether(
        &self,
        device: &Device,
        command: vk::CommandBuffer,
        destination: vk::ImageView,
        slot: &GlowSlot,
        extent: vk::Extent2D,
        quarter: vk::Extent2D,
        window: WorldScreenWindow,
        frame: WorldNetherFrame,
    ) {
        let mut parameters = [0.; 20];
        parameters[..8].copy_from_slice(&[
            frame.angle,
            (8. * f64::from(1. / extent.height as f32) * f64::from(1. / extent.width as f32))
                as f32,
            0.,
            0.,
            extent.width as f32,
            extent.height as f32,
            quarter.width as f32,
            quarter.height as f32,
        ]);
        for (index, bytes) in frame.colors.as_chunks::<4>().0.iter().enumerate() {
            parameters[8 + index] = f32::from_bits(u32::from_le_bytes(*bytes));
        }
        // 7E9B10 reuses the same published mesh/constants for both normalized draws.
        self.record_target_data(
            device,
            command,
            &slot.box_target,
            quarter,
            self.nether_blur,
            slot.sets[0],
            parameters,
            150,
        );
        self.record_target_data(
            device,
            command,
            &slot.vertical,
            quarter,
            self.nether_blur,
            slot.sets[1],
            parameters,
            150,
        );
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
            self.nether_combine,
            self.pipeline_layout,
            slot.sets[3],
            effect_parameters(
                [frame.fade, 0., 0., 0.],
                sampling(extent, extent),
                sampling(quarter, extent),
            ),
            3,
        );
        // SAFETY: The composite's rendering scope is active.
        unsafe { device.cmd_end_rendering(command) };
    }
}
