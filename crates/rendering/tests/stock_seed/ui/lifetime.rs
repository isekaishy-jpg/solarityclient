//! Real queue coverage for appended glyph pixels, mesh growth and owner release.

#![allow(unsafe_code)]

use solarity_rendering::{
    UiMeshPlan, UiRenderBlend, UiRenderSource, UiSampledTexture, UiSamplerInfo, UiShaderSource,
    UiTextureAddressMode, VulkanBootstrap,
};
use std::error::Error;

#[test]
fn glyph_updates_mesh_growth_and_descriptor_retirement_preserve_pixels()
-> Result<(), Box<dyn Error>> {
    let _lock = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity UI resource lifetime", 64, 64)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: The instance enabled this live window's required extensions.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    // SAFETY: SDL transfers sole surface ownership; the window outlives the renderer.
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (64, 64), 0) }?;
    renderer.configure_frame_waits(std::sync::Arc::new(super::super::device::GpuSignal(
        std::thread::current(),
    )))?;
    let sampler = renderer.prepare_ui_sampler(UiSamplerInfo::new(
        UiTextureAddressMode::Clamp,
        UiTextureAddressMode::Clamp,
    ))?;
    let pipeline = renderer.prepare_ui_pipeline(UiShaderSource::Texture, UiRenderBlend::Alpha)?;
    let baseline = renderer.resource_usage();
    for generation in 0..16 {
        let mut pixels = vec![255; 16 * 16 * 4];
        let glyph = renderer.upload_ui_glyph_texture(20_000 + generation, (16, 16), &pixels)?;
        let glyph_owner = renderer.retain_ui_glyph_texture(glyph)?;
        let set = renderer.prepare_ui_texture_sets(&[UiSampledTexture::glyph(glyph, sampler)])?[0];
        let mut mesh = None;
        let mut mesh_owner = None;
        for count in [1, 3, 17, 65, 2] {
            let plan = UiMeshPlan::prepare(
                [64.0; 2],
                (0..count).map(|_| {
                    super::quad(
                        1,
                        UiRenderSource::GlyphAtlas(20_000 + generation),
                        [0., 0., 64., 64.],
                    )
                }),
            )?;
            let handle = if let Some(handle) = mesh {
                renderer.replace_ui_mesh(handle, &plan)?;
                handle
            } else {
                let handle = renderer.upload_ui_mesh(&plan)?;
                mesh_owner = Some(renderer.retain_ui_mesh(handle)?);
                mesh = Some(handle);
                handle
            };
            let draw = renderer.prepare_ui_draw(handle, pipeline, Some(set), &plan, 0)?;
            renderer.request_frame_capture()?;
            renderer.present_ui_with_overlay_serviced(
                [64.0; 2],
                &[draw],
                &[],
                &mut super::super::device::service_gpu,
            )?;
            let capture = renderer
                .take_captured_frame()?
                .ok_or("missing UI capture")?;
            let at = (32 * 64 + 32) * 4;
            assert_eq!(&capture.rgba8()[at..at + 4], &[255; 4]);
        }
        // Change only a central rectangle after its previous sampled use.
        for y in 4..12 {
            for x in 4..12 {
                pixels[(y * 16 + x) * 4..(y * 16 + x) * 4 + 4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        renderer.update_ui_glyph_texture(glyph, (16, 16), &pixels, &[[4, 4, 8, 8]])?;
        let plan = UiMeshPlan::prepare(
            [64.0; 2],
            [super::quad(
                1,
                UiRenderSource::GlyphAtlas(20_000 + generation),
                [0., 0., 64., 64.],
            )]
            .into_iter(),
        )?;
        let handle = mesh.ok_or("missing retained mesh")?;
        renderer.replace_ui_mesh(handle, &plan)?;
        let draw = renderer.prepare_ui_draw(handle, pipeline, Some(set), &plan, 0)?;
        renderer.request_frame_capture()?;
        renderer.present_ui_with_overlay_serviced(
            [64.0; 2],
            &[draw],
            &[],
            &mut super::super::device::service_gpu,
        )?;
        let capture = renderer
            .take_captured_frame()?
            .ok_or("missing updated UI capture")?;
        let center = (32 * 64 + 32) * 4;
        assert_eq!(&capture.rgba8()[center..center + 4], &[255, 0, 0, 255]);
        let corner = (4 * 64 + 4) * 4;
        assert_eq!(&capture.rgba8()[corner..corner + 4], &[255; 4]);
        drop(mesh_owner);
        drop(glyph_owner);
        renderer.present_clear([64.0; 2])?;
        assert!(renderer.ui_mesh_info(handle).is_none());
        assert!(renderer.ui_glyph_texture_info(glyph).is_none());
        assert!(renderer.ui_texture_set_info(set).is_none());
        let usage = renderer.resource_usage();
        assert_eq!(usage.ui_meshes, baseline.ui_meshes);
        assert_eq!(usage.glyph_pages, baseline.glyph_pages);
        assert_eq!(usage.ui_texture_sets, baseline.ui_texture_sets);
    }
    Ok(())
}
