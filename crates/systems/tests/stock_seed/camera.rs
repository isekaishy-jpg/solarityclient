//! External stock-compatibility tests for build-12340 player camera policy.

use std::error::Error;

use glam::Vec3;
use solarity_ecs::{PlayerViewState, WorldTransform};
use solarity_systems::{
    CameraSubjectGeometry, CameraSubjectHeightError, CameraSubjectHeightSource,
    PlayerCameraPoseError, resolve_camera_subject_height, resolve_player_camera_pose,
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
