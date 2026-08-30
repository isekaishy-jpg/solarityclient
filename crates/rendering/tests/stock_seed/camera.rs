//! External stock-compatibility tests for the build-12340 world camera.

use std::error::Error;

use glam::{Vec3, Vec4};
use solarity_rendering::{
    WORLD_DEPTH_MAXIMUM, WORLD_DEPTH_MINIMUM, WORLD_NEAR_CLIP,
    WORLD_VERTICAL_FIELD_OF_VIEW_RADIANS, WorldCamera, WorldCameraError, WorldFrustum,
    WorldScreenWindow,
};

/// Stock world constants and Vulkan clip correction produce the shared frame.
#[test]
fn stock_world_camera_builds_vulkan_projection() -> Result<(), Box<dyn Error>> {
    let camera = WorldCamera::stock(Vec3::ZERO, Vec3::X, Vec3::Z, 2_000.0);
    let frame = camera.frame(16.0 / 9.0)?;

    assert_eq!(camera.near_clip(), WORLD_NEAR_CLIP);
    assert_eq!(WORLD_DEPTH_MINIMUM, 0.0);
    assert_eq!(WORLD_DEPTH_MAXIMUM, 0.94);
    assert!(
        (camera.vertical_field_of_view_radians() - WORLD_VERTICAL_FIELD_OF_VIEW_RADIANS).abs()
            < 1.0e-7
    );
    assert_eq!(frame.forward(), Vec3::X);
    assert_eq!(frame.right(), -Vec3::Y);
    assert_eq!(frame.up(), Vec3::Z);

    let near_clip = frame.view_projection() * Vec4::new(WORLD_NEAR_CLIP, 0.0, 0.0, 1.0);
    let far_clip = frame.view_projection() * Vec4::new(2_000.0, 0.0, 0.0, 1.0);
    assert!((near_clip.z / near_clip.w).abs() < 1.0e-5);
    assert!((far_clip.z / far_clip.w - 1.0).abs() < 1.0e-5);

    // Positive world Z is screen-up before Vulkan's required clip-Y reversal.
    let above = frame.view_projection() * Vec4::new(1.0, 0.0, 0.25, 1.0);
    assert!(above.y / above.w < 0.0);
    Ok(())
}

/// The eye-based frustum rejects geometry behind or outside the view wedge.
#[test]
fn world_frustum_tests_spheres_and_oriented_boxes() -> Result<(), Box<dyn Error>> {
    let frame = WorldCamera::stock(Vec3::ZERO, Vec3::X, Vec3::Z, 100.0).frame(1.0)?;
    let frustum = WorldFrustum::new(frame, WorldScreenWindow::FULL)?;

    assert!(frustum.contains_sphere(Vec3::new(10.0, 0.0, 0.0), 1.0)?);
    assert!(!frustum.contains_sphere(Vec3::new(-2.0, 0.0, 0.0), 0.5)?);
    assert!(!frustum.contains_sphere(Vec3::new(10.0, 20.0, 0.0), 0.5)?);
    assert!(frustum.intersects_box(
        Vec3::new(10.0, 0.0, 0.0),
        Vec3::new(0.5, 0.0, 0.0),
        Vec3::new(0.0, 2.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
    )?);
    Ok(())
}

/// Invalid camera bases and portal windows are errors, not hidden fallbacks.
#[test]
fn world_camera_rejects_degenerate_inputs() -> Result<(), Box<dyn Error>> {
    let parallel_up = WorldCamera::stock(Vec3::ZERO, Vec3::X, Vec3::X, 100.0);
    assert_eq!(parallel_up.frame(1.0), Err(WorldCameraError::UpDirection));
    let frame = WorldCamera::stock(Vec3::ZERO, Vec3::X, Vec3::Z, 100.0).frame(1.0)?;
    assert_eq!(
        WorldFrustum::new(frame, WorldScreenWindow::new(1.0, -1.0, -1.0, 1.0)),
        Err(WorldCameraError::ScreenWindow)
    );
    Ok(())
}
