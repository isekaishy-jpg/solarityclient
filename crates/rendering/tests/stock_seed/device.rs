//! External stock-compatibility tests for `rendering/device` belong here.

#![allow(unsafe_code)]

use std::error::Error;

use solarity_rendering::{CinematicFrameIdentity, VulkanBootstrap};

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
