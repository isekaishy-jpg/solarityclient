//! External stock-compatibility tests for build-12340 player camera policy.

use std::convert::Infallible;
use std::error::Error;

use glam::Vec3;
use solarity_ecs::{PlayerViewState, WorldTransform};
use solarity_systems::{
    CameraSubjectGeometry, CameraSubjectHeightError, CameraSubjectHeightSource,
    PlayerCameraObstructionError, PlayerCameraPoseError, PlayerCameraWaterError,
    resolve_camera_subject_height, resolve_player_camera_obstruction, resolve_player_camera_pose,
    resolve_player_camera_water_collision,
};

/// The default saved view produces stock's distinct eye, target, pivot, and subject.
#[test]
fn default_view_resolves_the_stock_player_orbit() -> Result<(), Box<dyn Error>> {
    let transform = WorldTransform::new(Vec3::new(10.0, 20.0, 30.0), 0.0);
    let height = resolve_camera_subject_height(CameraSubjectGeometry::new(Some(1.75), 99.0, 1.0))?;
    let pose = resolve_player_camera_pose(transform, PlayerViewState::STOCK_VIEW_2, height)?;

    assert!(
        pose.subject()
            .abs_diff_eq(Vec3::new(10.0, 20.0, 30.0), 0.000_001)
    );
    assert!(
        pose.orbit_pivot()
            .abs_diff_eq(Vec3::new(10.0, 20.0, 31.847_221), 0.000_01)
    );
    assert!(
        pose.eye()
            .abs_diff_eq(Vec3::new(4.534_317, 20.0, 32.810_97), 0.000_01)
    );
    assert!(((pose.target() - pose.eye()).length() - 1.0).abs() < 0.000_001);
    assert!((pose.up().length() - 1.0).abs() < 0.000_001);
    assert!((pose.target() - pose.eye()).dot(pose.up()).abs() < 0.000_001);
    Ok(())
}

/// Non-finite saved state is rejected rather than reaching renderer matrices.
#[test]
fn invalid_player_view_has_no_camera_fallback() -> Result<(), Box<dyn Error>> {
    let height = resolve_camera_subject_height(CameraSubjectGeometry::new(Some(1.75), 2.0, 1.0))?;
    let view = PlayerViewState::new(5.55, f32::NAN, 0.0, 2);

    assert_eq!(
        resolve_player_camera_pose(WorldTransform::new(Vec3::ZERO, 0.0), view, height),
        Err(PlayerCameraPoseError::NonFiniteView)
    );
    Ok(())
}

/// The authored Breath attachment is stable and takes priority over bounds.
#[test]
fn breath_attachment_drives_character_camera_height() -> Result<(), Box<dyn Error>> {
    let height = resolve_camera_subject_height(CameraSubjectGeometry::new(Some(1.75), 99.0, 1.0))?;

    assert_eq!(height.source(), CameraSubjectHeightSource::BreathAttachment);
    assert!((height.value() - 1.847_222_2).abs() < 0.000_001);
    Ok(())
}

/// A missing lookup slot selects stock's 99-percent model-sphere branch.
#[test]
fn absent_breath_attachment_uses_model_sphere() -> Result<(), Box<dyn Error>> {
    let height = resolve_camera_subject_height(CameraSubjectGeometry::new(None, 2.0, 1.5))?;

    assert_eq!(height.source(), CameraSubjectHeightSource::ModelSphere);
    assert!((height.value() - 2.97).abs() < 0.000_001);
    Ok(())
}

/// Present invalid authored inputs are not replaced by the other branch.
#[test]
fn invalid_camera_geometry_has_no_quality_fallback() {
    assert_eq!(
        resolve_camera_subject_height(CameraSubjectGeometry::new(Some(f32::NAN), 2.0, 1.0)),
        Err(CameraSubjectHeightError::NonFiniteBreathAttachment)
    );
    assert_eq!(
        resolve_camera_subject_height(CameraSubjectGeometry::new(None, -1.0, 1.0)),
        Err(CameraSubjectHeightError::InvalidModelSphere)
    );
    assert_eq!(
        resolve_camera_subject_height(CameraSubjectGeometry::new(None, 2.0, f32::INFINITY)),
        Err(CameraSubjectHeightError::NonFiniteScale)
    );
}

/// Stock clamps both extremely small and extremely tall unit pivots.
#[test]
fn camera_height_obeys_stock_clamps() -> Result<(), Box<dyn Error>> {
    let minimum = resolve_camera_subject_height(CameraSubjectGeometry::new(None, 0.0, 0.0))?;
    let maximum = resolve_camera_subject_height(CameraSubjectGeometry::new(Some(100.0), 1.0, 3.0))?;

    assert_eq!(minimum.value(), 0.833_333_3);
    assert_eq!(maximum.value(), 15.0);
    Ok(())
}

/// Center obstruction retreats the eye while smart pivot preserves view direction.
#[test]
fn camera_obstruction_resolves_the_stock_swept_volume() -> Result<(), Box<dyn Error>> {
    let transform = WorldTransform::new(Vec3::new(10.0, 20.0, 30.0), 0.0);
    let height = resolve_camera_subject_height(CameraSubjectGeometry::new(Some(1.75), 2.0, 1.0))?;
    let pose = resolve_player_camera_pose(transform, PlayerViewState::STOCK_VIEW_2, height)?;
    let requested_direction = pose.target() - pose.eye();
    let mut trace_count = 0;
    let resolved = resolve_player_camera_obstruction(
        pose,
        16.0 / 9.0,
        true,
        |_start, _end, _maximum| -> Result<Option<f32>, Infallible> {
            trace_count += 1;
            Ok((trace_count == 1).then_some(0.5))
        },
    )?;

    let ray_length = (pose.eye() - pose.orbit_pivot()).length();
    let expected_fraction = 0.5 - 0.111_111_11 / ray_length;
    let expected_eye = pose.orbit_pivot() + (pose.eye() - pose.orbit_pivot()) * expected_fraction;
    assert_eq!(trace_count, 9);
    assert!(resolved.eye().abs_diff_eq(expected_eye, 0.000_001));
    assert!((resolved.target() - resolved.eye()).abs_diff_eq(requested_direction, 0.000_001));
    Ok(())
}

/// Trace providers cannot return fractions outside the requested interval.
#[test]
fn camera_obstruction_rejects_invalid_provider_fraction() -> Result<(), Box<dyn Error>> {
    let height = resolve_camera_subject_height(CameraSubjectGeometry::new(None, 2.0, 1.0))?;
    let pose = resolve_player_camera_pose(
        WorldTransform::new(Vec3::ZERO, 0.0),
        PlayerViewState::STOCK_VIEW_2,
        height,
    )?;
    let result = resolve_player_camera_obstruction(
        pose,
        1.0,
        true,
        |_start, _end, maximum| -> Result<Option<f32>, Infallible> { Ok(Some(maximum + 0.1)) },
    );

    assert!(matches!(
        result,
        Err(PlayerCameraObstructionError::InvalidTraceFraction)
    ));
    Ok(())
}

/// Water collision keeps the final eye on the followed pivot's side of water.
#[test]
fn camera_water_collision_applies_stock_clearance() -> Result<(), Box<dyn Error>> {
    let height = resolve_camera_subject_height(CameraSubjectGeometry::new(None, 2.0, 1.0))?;
    let pose = resolve_player_camera_pose(
        WorldTransform::new(Vec3::ZERO, 0.0),
        PlayerViewState::STOCK_VIEW_2,
        height,
    )?;
    let requested_direction = pose.target() - pose.eye();
    let mut query = 0;
    let dry = resolve_player_camera_water_collision(
        pose,
        true,
        true,
        |_x, _y, reference| -> Result<Option<f32>, Infallible> {
            query += 1;
            Ok(Some(if query == 1 {
                reference - 1.0
            } else {
                reference + 2.0
            }))
        },
    )?;
    assert_eq!(query, 2);
    assert!((dry.eye().z - (pose.eye().z + 2.05)).abs() < 0.000_001);
    assert!((dry.target() - dry.eye()).abs_diff_eq(requested_direction, 0.000_001));

    query = 0;
    let submerged = resolve_player_camera_water_collision(
        pose,
        true,
        false,
        |_x, _y, reference| -> Result<Option<f32>, Infallible> {
            query += 1;
            Ok(Some(if query == 1 {
                reference + 1.0
            } else {
                reference - 2.0
            }))
        },
    )?;
    assert!((submerged.eye().z - (pose.eye().z - 2.05)).abs() < 0.000_001);
    assert!(
        (submerged.target() - submerged.eye())
            .normalize()
            .abs_diff_eq(
                (pose.orbit_pivot() - submerged.eye()).normalize(),
                0.000_001
            )
    );
    Ok(())
}

/// Disabled water collision is inert and invalid provider heights are rejected.
#[test]
fn camera_water_collision_has_no_surface_fallback() -> Result<(), Box<dyn Error>> {
    let height = resolve_camera_subject_height(CameraSubjectGeometry::new(None, 2.0, 1.0))?;
    let pose = resolve_player_camera_pose(
        WorldTransform::new(Vec3::ZERO, 0.0),
        PlayerViewState::STOCK_VIEW_2,
        height,
    )?;
    let mut query_count = 0;
    let disabled = resolve_player_camera_water_collision(
        pose,
        false,
        true,
        |_x, _y, _z| -> Result<Option<f32>, Infallible> {
            query_count += 1;
            Ok(Some(f32::NAN))
        },
    )?;
    assert_eq!(disabled, pose);
    assert_eq!(query_count, 0);

    assert!(matches!(
        resolve_player_camera_water_collision(
            pose,
            true,
            true,
            |_x, _y, _z| -> Result<Option<f32>, Infallible> { Ok(Some(f32::NAN)) },
        ),
        Err(PlayerCameraWaterError::InvalidSurfaceHeight)
    ));
    Ok(())
}
