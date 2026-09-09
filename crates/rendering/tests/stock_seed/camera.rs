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
        (camera
            .vertical_field_of_view_radians()
            .ok_or("missing perspective FOV")?
            - WORLD_VERTICAL_FIELD_OF_VIEW_RADIANS)
            .abs()
            < 1.0e-7
    );
    assert_eq!(frame.forward(), Vec3::X);
    assert_eq!(frame.right(), -Vec3::Y);
    assert_eq!(frame.up(), Vec3::Z);

    let near_clip = frame.view_projection() * Vec4::new(WORLD_NEAR_CLIP, 0.0, 0.0, 1.0);
    let far_clip = frame.view_projection() * Vec4::new(2_000.0, 0.0, 0.0, 1.0);
    assert!((near_clip.z / near_clip.w).abs() < 1.0e-5);
    assert!((far_clip.z / far_clip.w - 1.0).abs() < 1.0e-5);

    // The renderer's negative-height viewport maps positive clip Y upward.
    let above = frame.view_projection() * Vec4::new(1.0, 0.0, 0.25, 1.0);
    assert!(above.y / above.w > 0.0);
    Ok(())
}

/// Player cameras keep stock's subject and collision pivot distinct from target.
#[test]
fn followed_world_camera_retains_distinct_subject_positions() -> Result<(), Box<dyn Error>> {
    let camera = WorldCamera::stock_following(
        Vec3::new(4.0, 2.0, 3.0),
        Vec3::new(5.0, 2.0, 3.0),
        Vec3::Z,
        Vec3::new(10.0, 2.0, 3.8),
        Vec3::new(10.0, 2.0, 2.0),
        777.0,
    );
    let subject = camera
        .subject()
        .ok_or("player camera omitted its subject")?;

    assert_eq!(subject.orbit_pivot(), Vec3::new(10.0, 2.0, 3.8));
    assert_eq!(subject.position(), Vec3::new(10.0, 2.0, 2.0));
    assert_eq!(camera.frame(1.0)?.camera(), camera);
    Ok(())
}

/// 607DB8's retained direction prevents world-coordinate rounding from turning
/// the camera as its position changes; depth consumers still retain the target.
#[test]
fn retained_camera_direction_preserves_view_basis_at_large_coordinates()
-> Result<(), Box<dyn Error>> {
    let forward = Vec3::new(0.7234567, -0.5345678, 0.3456789).normalize();
    let reference = WorldCamera::stock(Vec3::ZERO, forward, Vec3::Z, 5000.)
        .with_view_direction(forward)
        .frame(16. / 9.)?;
    for eye in [Vec3::new(15000., -14000., 2500.), Vec3::splat(33_554_432.)] {
        let target = eye + forward;
        assert_ne!(target - eye, forward);
        let camera = WorldCamera::stock(eye, target, Vec3::Z, 5000.).with_view_direction(forward);
        let frame = camera.frame(16. / 9.)?;
        assert_eq!(camera.target(), target);
        assert_eq!(camera.view_direction(), forward);
        assert_eq!(frame.forward(), reference.forward());
        assert_eq!(frame.right(), reference.right());
        assert_eq!(frame.up(), reference.up());
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            assert_eq!(
                frame.view().transform_vector3(axis),
                reference.view().transform_vector3(axis)
            );
        }
    }
    assert_eq!(
        WorldCamera::stock(Vec3::ZERO, Vec3::X, Vec3::Z, 100.)
            .with_view_direction(Vec3::splat(f32::NAN))
            .frame(1.),
        Err(WorldCameraError::NonFiniteBasis)
    );
    assert_eq!(
        WorldCamera::stock(Vec3::ZERO, Vec3::X, Vec3::Z, 100.)
            .with_view_direction(Vec3::ZERO)
            .frame(1.),
        Err(WorldCameraError::ViewDirection)
    );
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

/// Parallel UI cameras have asymmetric bounds, signed depth, and constant
/// screen size. Visibility must use the same volume at every depth.
#[test]
fn orthographic_camera_projection_and_visibility_share_signed_volume() -> Result<(), Box<dyn Error>>
{
    let eye = Vec3::new(100.0, -200.0, 300.0);
    let camera = WorldCamera::orthographic(
        eye,
        eye - Vec3::X,
        Vec3::Z,
        [-2.0, 6.0],
        [-3.0, 1.0],
        -500.0,
        500.0,
    );
    let frame = camera.frame(2.0)?;
    let frustum = WorldFrustum::new(frame, WorldScreenWindow::FULL)?;
    assert_eq!(camera.vertical_field_of_view_radians(), None);
    let point = |x, y, depth| eye + frame.right() * x + frame.up() * y + frame.forward() * depth;
    for depth in [-500.0, -150.0, 0.0, 250.0, 500.0] {
        let center = point(2.0, -1.0, depth);
        let clip = frame.view_projection() * center.extend(1.0);
        assert_eq!(clip.w, 1.0);
        assert!(clip.x.abs() < 0.0001 && clip.y.abs() < 0.0001);
        assert!((clip.z - (depth + 500.0) / 1_000.0).abs() < 0.0001);
        assert!(frustum.contains_sphere(center, 0.0)?);
        assert!(frustum.contains_sphere(point(6.5, -1.0, depth), 0.5)?);
        assert!(!frustum.contains_sphere(point(6.6, -1.0, depth), 0.5)?);
        assert!(!frustum.contains_sphere(point(2.0, 1.6, depth), 0.5)?);
    }
    assert!(!frustum.contains_sphere(point(2.0, -1.0, -501.0), 0.5)?);
    assert!(!frustum.contains_sphere(point(2.0, -1.0, 501.0), 0.5)?);
    let clipped = WorldFrustum::new(frame, WorldScreenWindow::new(-0.5, -0.25, 0.5, 0.75))?;
    assert!(clipped.contains_sphere(point(3.0, 0.0, -100.0), 0.0)?);
    assert!(!clipped.contains_sphere(point(5.0, 0.0, -100.0), 0.0)?);
    assert!(clipped.intersects_box(
        point(4.25, 0.0, -100.0),
        frame.right() * 0.5,
        frame.up() * 0.25,
        frame.forward(),
    )?);
    assert!(!clipped.intersects_box(
        point(5.0, 0.0, -100.0),
        frame.right() * 0.5,
        frame.up() * 0.25,
        frame.forward(),
    )?);
    for (horizontal, vertical, near, far, error) in [
        (
            [0.0, 0.0],
            [0.0, 1.0],
            -1.0,
            1.0,
            WorldCameraError::OrthographicBounds,
        ),
        (
            [0.0, 1.0],
            [1.0, 0.0],
            -1.0,
            1.0,
            WorldCameraError::OrthographicBounds,
        ),
        (
            [0.0, f32::NAN],
            [0.0, 1.0],
            -1.0,
            1.0,
            WorldCameraError::OrthographicBounds,
        ),
        (
            [0.0, 1.0],
            [0.0, 1.0],
            1.0,
            -1.0,
            WorldCameraError::ClipRange,
        ),
    ] {
        assert_eq!(
            WorldCamera::orthographic(eye, eye - Vec3::X, Vec3::Z, horizontal, vertical, near, far)
                .frame(1.0),
            Err(error),
        );
    }
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
    let invalid_subject = WorldCamera::stock_following(
        Vec3::ZERO,
        Vec3::X,
        Vec3::Z,
        Vec3::new(f32::NAN, 0.0, 0.0),
        Vec3::ZERO,
        100.0,
    );
    assert_eq!(
        invalid_subject.frame(1.0),
        Err(WorldCameraError::NonFiniteSubject)
    );
    Ok(())
}
