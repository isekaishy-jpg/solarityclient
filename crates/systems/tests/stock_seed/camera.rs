//! External stock-compatibility tests for build-12340 player camera policy.

use std::error::Error;

use solarity_systems::{
    CameraSubjectGeometry, CameraSubjectHeightError, CameraSubjectHeightSource,
    resolve_camera_subject_height,
};

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
