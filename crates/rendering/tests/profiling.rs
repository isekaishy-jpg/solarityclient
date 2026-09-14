//! F10 GPU samples must retire through normal presentation fences on a hidden surface.

#![allow(unsafe_code)] // The hidden SDL surface transfers sole ownership to Vulkan.

use std::error::Error;

use solarity_profiling::{Capture, begin_frame};
use solarity_rendering::VulkanBootstrap;

#[test]
fn ui_timestamps_survive_capture_restart_and_slot_reuse() -> Result<(), Box<dyn Error>> {
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity profiling test", 64, 64)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: The live window supplies this instance's enabled surface extensions.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    // SAFETY: Renderer takes ownership and is dropped before the hidden window.
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (64, 64), 0) }?;
    let root =
        std::env::temp_dir().join(format!("solarity-gpu-profile-test-{}", std::process::id()));
    let mut capture = Capture::new(&root, "fixture=true".to_owned());
    for _ in 0..2 {
        let (_, path) = capture.toggle()?;
        for _ in 0..260 {
            let _frame = begin_frame();
            renderer.present_clear([64., 64.])?;
        }
        capture.shutdown()?;
        let summary = std::fs::read_to_string(path.with_extension("summary.csv"))?;
        assert!(summary.contains("rendering.device.vulkan_ui_frame.mod.present_inner"));
        // This real Vulkan test requires the timestamp-capable test adapter.
        assert!(summary.contains("rendering.gpu.ui_only"));
    }
    renderer.present_clear([64., 64.])?;
    std::fs::remove_dir_all(root)?;
    Ok(())
}
