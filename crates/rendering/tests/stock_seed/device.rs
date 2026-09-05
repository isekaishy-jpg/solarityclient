//! External stock-compatibility tests for `rendering/device` belong here.

#![allow(unsafe_code)]

use std::error::Error;

use solarity_rendering::{CinematicFrameIdentity, VulkanBootstrap, VulkanError};

/// Stock's 0x0095EBF0 update retains decoded surfaces until frame identity changes.
#[test]
fn cinematic_frame_reuses_the_authored_source_between_display_refreshes()
-> Result<(), Box<dyn Error>> {
    let _sdl_test = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let mut window_builder = video.window("Solarity cinematic frame test", 64, 64);
    window_builder.vulkan().hidden();
    let window = window_builder.build()?;
    let extensions = window.vulkan_instance_extensions()?;
    let bootstrap = VulkanBootstrap::start(&extensions)?;
    // SAFETY: The bootstrap enabled this live window's exact extensions.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    // SAFETY: SDL transfers its sole surface ownership into this renderer.
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (64, 64), 0) }?;
    let pixels = vec![0x7F_u8; 32 * 16 * 4];
    let identity = CinematicFrameIdentity::new(7, 11);

    renderer.present_cinematic_rgba8(identity, (32, 16), &pixels)?;
    assert_eq!(renderer.report().presented_source_reused(), Some(false));
    renderer.present_cinematic_rgba8(identity, (32, 16), &pixels)?;
    assert_eq!(renderer.report().presented_source_reused(), Some(true));
    renderer.present_cinematic_rgba8(CinematicFrameIdentity::new(7, 12), (32, 16), &pixels)?;
    assert_eq!(renderer.report().presented_source_reused(), Some(false));
    Ok(())
}

/// Actual GPU pixels prove channel order, row order, one-shot lifetime, and clear capture.
#[test]
fn framebuffer_capture_retains_the_requested_present_until_collection() -> Result<(), Box<dyn Error>>
{
    let _sdl_test = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let mut window_builder = video.window("Solarity framebuffer capture test", 64, 64);
    window_builder.vulkan().hidden();
    let window = window_builder.build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: The bootstrap enabled the live window's exact required extensions.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    // SAFETY: The window outlives the renderer receiving sole surface ownership.
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (64, 64), 0) }?;
    let extent = renderer.report().extent();
    let mut pixels = Vec::with_capacity((extent.0 * extent.1 * 4) as usize);
    for y in 0..extent.1 {
        for x in 0..extent.0 {
            pixels.extend_from_slice(&[x as u8, 91, y as u8, 255]);
        }
    }
    assert!(renderer.take_captured_frame()?.is_none());
    renderer.request_frame_capture()?;
    assert!(renderer.take_captured_frame()?.is_none());
    assert!(matches!(
        renderer.request_frame_capture(),
        Err(VulkanError::FrameCaptureBusy)
    ));
    assert!(renderer.present_rgba8(extent, &[]).is_err());
    renderer.present_rgba8(extent, &pixels)?;
    // A subsequent successfully presented frame must not overwrite the first.
    let logical_extent = [extent.0 as f32, extent.1 as f32];
    renderer.present_clear(logical_extent)?;
    let captured = renderer.take_captured_frame()?.ok_or("capture is absent")?;
    assert_eq!(captured.extent(), extent);
    assert_eq!(captured.rgba8(), pixels);
    assert!(renderer.take_captured_frame()?.is_none());

    renderer.request_frame_capture()?;
    renderer.present_clear(logical_extent)?;
    let clear = renderer
        .take_captured_frame()?
        .ok_or("clear capture is absent")?;
    assert_eq!(clear.extent(), extent);
    assert!(
        clear
            .rgba8()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [0, 0, 0, 255])
    );
    // An unsubmitted request also has a valid teardown path.
    renderer.request_frame_capture()?;
    Ok(())
}
