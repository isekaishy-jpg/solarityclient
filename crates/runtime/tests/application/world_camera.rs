//! Terrain-demand camera reuse must preserve the later presentation clock.

use std::error::Error;

use glam::Vec3;
use solarity_ecs::{PlayerViewState, WorldTransform};
use solarity_rendering::WorldCamera;
use solarity_systems::{
    CameraSubjectGeometry, PlayerCameraHeightState, PlayerCameraObstructionSettings,
    PlayerCameraPose, resolve_camera_subject_height, resolve_mounted_player_camera_pose,
};

use super::{CameraInputs, ResolvedCameraFrame};

/// 603D30 advances collision recovery even when transform, view and scene stay
/// fixed. The pre-sample pose therefore cannot stand in for a clock dependency.
#[test]
fn presentation_after_streaming_samples_current_collision_recovery() -> Result<(), Box<dyn Error>> {
    let base = resolve_camera_subject_height(CameraSubjectGeometry::new(None, 2.0, 1.0))?;
    let mut heights = PlayerCameraHeightState::new(base);
    heights.obstructed(0.5, 1000)?;
    let transform = WorldTransform::new(Vec3::ZERO, 0.0);
    let view = PlayerViewState::new(5.55, 0.2, 0.0, 2);
    let pose = resolve_mounted_player_camera_pose(transform, view, heights.sample_collision(1500))?;
    let mut cached = frame(pose, 1500)?;

    assert!(cached.camera_for(Some(&inputs(pose)), 1500).is_some());
    // Presentation has not sampled height yet: all spatial inputs still match.
    assert!(cached.camera_for(Some(&inputs(pose)), 1516).is_none());
    let current =
        resolve_mounted_player_camera_pose(transform, view, heights.sample_collision(1516))?;
    assert!(
        cached
            .after_clock_sample(Some(&inputs(current)), 1516)
            .is_none()
    );
    let presentation = frame(current, 1516)?;
    assert!(presentation.camera.camera().position().z > cached.camera.camera().position().z);
    Ok(())
}

/// Millisecond equality remains valid across wrap; numeric ordering would retain
/// the pre-wrap camera incorrectly when SDL's wrapping clock starts again.
#[test]
fn camera_reuse_rejects_clock_wrap_and_changed_providers() -> Result<(), Box<dyn Error>> {
    let base = resolve_camera_subject_height(CameraSubjectGeometry::new(None, 2.0, 1.0))?;
    let pose = resolve_mounted_player_camera_pose(
        WorldTransform::new(Vec3::ZERO, 0.0),
        PlayerViewState::default(),
        PlayerCameraHeightState::new(base).sample_collision(u32::MAX),
    )?;
    let mut cached = frame(pose, u32::MAX)?;
    assert!(cached.camera_for(Some(&inputs(pose)), u32::MAX).is_some());
    assert!(cached.camera_for(Some(&inputs(pose)), 0).is_none());
    assert!(cached.camera_for(None, u32::MAX).is_none());
    assert!(cached.after_clock_sample(None, 0).is_none());
    let mut changed = inputs(pose);
    changed.terrain_revision += 1;
    assert!(cached.camera_for(Some(&changed), u32::MAX).is_none());
    assert!(cached.after_clock_sample(Some(&changed), 0).is_none());
    changed = inputs(pose);
    changed.pose = pose.with_eye(pose.eye() + Vec3::Y)?;
    assert!(cached.camera_for(Some(&changed), u32::MAX).is_none());
    assert!(cached.after_clock_sample(Some(&changed), 0).is_none());
    Ok(())
}

/// An idle camera still samples its clock but does not repeat spatial queries
/// solely because terrain/UI service crossed a millisecond boundary.
#[test]
fn unchanged_current_sample_reuses_collision_across_ticks() -> Result<(), Box<dyn Error>> {
    let base = resolve_camera_subject_height(CameraSubjectGeometry::new(None, 2.0, 1.0))?;
    let mut heights = PlayerCameraHeightState::new(base);
    let transform = WorldTransform::new(Vec3::ZERO, 0.0);
    let view = PlayerViewState::default();
    let pose = resolve_mounted_player_camera_pose(transform, view, heights.sample_collision(1000))?;
    let mut cached = frame(pose, 1000)?;
    assert!(cached.camera_for(Some(&inputs(pose)), 1016).is_none());
    let current =
        resolve_mounted_player_camera_pose(transform, view, heights.sample_collision(1016))?;
    let reused = cached.after_clock_sample(Some(&inputs(current)), 1016);
    assert_eq!(reused.map(|frame| frame.view()), Some(cached.camera.view()));
    assert!(cached.camera_for(Some(&inputs(current)), 1016).is_some());
    Ok(())
}

/// Fixed providers isolate the clock dependency from terrain and input changes.
fn inputs(pose: PlayerCameraPose) -> CameraInputs {
    CameraInputs {
        pose,
        settings: PlayerCameraObstructionSettings::default(),
        terrain_revision: 7,
        extent: (2560, 1440),
        far_clip: 1277.0,
        pivot_pitch: 0.0,
    }
}

/// Uses the same stock projection as the runtime camera composition boundary.
fn frame(
    pose: PlayerCameraPose,
    sampled_at_ms: u32,
) -> Result<ResolvedCameraFrame, Box<dyn Error>> {
    let inputs = inputs(pose);
    let camera = WorldCamera::stock_following(
        pose.eye(),
        pose.target(),
        pose.up(),
        pose.orbit_pivot(),
        pose.subject(),
        inputs.far_clip,
    )
    .with_view_direction(pose.forward())
    .frame(16.0 / 9.0)?;
    Ok(ResolvedCameraFrame {
        sampled_at_ms,
        inputs,
        camera,
    })
}
