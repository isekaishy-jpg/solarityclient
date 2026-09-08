//! External stock-compatibility tests for build-12340 player camera policy.

use std::cell::RefCell;
use std::convert::Infallible;
use std::error::Error;

#[test]
fn camera_volumes_match_original_triangle_clipping() -> Result<(), Box<dyn Error>> {
    use solarity_systems::{
        PlayerCameraVolumeError, PlayerCameraVolumeKind, resolve_player_camera_volume,
    };
    for (line_number, line) in include_str!("../fixtures/camera-volume-native.txt")
        .lines()
        .enumerate()
    {
        if line.starts_with('#') {
            continue;
        }
        let groups = line
            .split('|')
            .map(|group| {
                group
                    .split_whitespace()
                    .map(|word| u32::from_str_radix(word, 16))
                    .collect::<Result<Vec<_>, _>>()
            })
            .collect::<Result<Vec<_>, _>>()?;
        let input = &groups[0];
        let float = |index| f32::from_bits(input[index]);
        let vector = |index| Vec3::new(float(index), float(index + 1), float(index + 2));
        let mut calls = 0;
        let result = resolve_player_camera_volume(
            float(6),
            vector(0),
            vector(3),
            float(7),
            input[8] & 0x20000 != 0,
            |volume, kind| {
                let mask = match kind {
                    PlayerCameraVolumeKind::Water => 0x20000,
                    PlayerCameraVolumeKind::Solid => 0x100171,
                };
                assert_eq!(mask, groups[2][calls * 25], "mask line {}", line_number + 1);
                for (actual, expected) in volume.corners().iter().flat_map(|p| p.to_array()).zip(
                    groups[2][calls * 25 + 1..calls * 25 + 25]
                        .iter()
                        .map(|word| f32::from_bits(*word)),
                ) {
                    assert!(
                        actual.to_bits() == expected.to_bits()
                            || (line_number > 320 && (actual - expected).abs() <= 0.000_02),
                        "corner line {}: {actual} != {expected}",
                        line_number + 1
                    );
                }
                calls += 1;
                if input[9] & mask != 0 {
                    volume.triangle_retreat([vector(10), vector(13), vector(16)])
                } else {
                    Ok::<_, PlayerCameraVolumeError>(None)
                }
            },
        )?;
        assert_eq!(calls * 25, groups[2].len());
        assert_eq!(
            result.is_some(),
            groups[1][0] != 0,
            "hit line {}",
            line_number + 1
        );
        let actual = result.unwrap_or(float(6));
        let expected = f32::from_bits(groups[1][1]);
        assert!(
            actual.to_bits() == expected.to_bits()
                || (line_number > 320 && (actual - expected).abs() <= 0.000_02),
            "distance line {}: {actual} != {expected}",
            line_number + 1
        );
    }
    Ok(())
}

use glam::Vec3;
use solarity_ecs::{PlayerViewState, WorldTransform};
use solarity_systems::{
    CameraSubjectGeometry, CameraSubjectHeightError, CameraSubjectHeightSource,
    MountCameraGeometry, MountCameraHeightError, PlayerCameraHeightState,
    PlayerCameraObstructionError, PlayerCameraPoseError, PlayerCameraWaterError,
    resolve_camera_subject_height, resolve_mounted_player_camera_pose,
    resolve_player_camera_obstruction, resolve_player_camera_pose,
    resolve_player_camera_water_collision,
};

#[test]
fn camera_water_classification_and_interface_match_original_execution() -> Result<(), Box<dyn Error>>
{
    use solarity_systems::{PlayerCameraLiquidState, resolve_player_camera_water_interface};
    for (line_number, line) in include_str!("../fixtures/camera-water-native.txt")
        .lines()
        .enumerate()
    {
        if line.starts_with('#') {
            continue;
        }
        let groups = line[2..]
            .split('|')
            .map(|group| {
                group
                    .split_whitespace()
                    .map(|word| u32::from_str_radix(word, 16))
                    .collect::<Result<Vec<_>, _>>()
            })
            .collect::<Result<Vec<_>, _>>()?;
        let input = &groups[0];
        let expected = &groups[1];
        let float = |index| f32::from_bits(input[index]);
        let vector = |index| Vec3::new(float(index), float(index + 1), float(index + 2));
        if line.starts_with('C') {
            let state = PlayerCameraLiquidState::sample(
                float(0),
                float(2),
                (input[3] != 0).then(|| float(1)),
            )?;
            let (depth, flags) = match state {
                PlayerCameraLiquidState::Absent => (0.0, 0),
                PlayerCameraLiquidState::Surface { depth } => (depth, 0x100000),
                PlayerCameraLiquidState::Submerged { depth } => (depth, 0x200000),
            };
            assert_eq!(
                [depth.to_bits(), (input[4] & !0x300000) | flags],
                expected.as_slice(),
                "line {}",
                line_number + 1
            );
        } else {
            let calls = RefCell::new(Vec::new());
            let eye = resolve_player_camera_water_interface(
                vector(0),
                vector(3),
                vector(6),
                float(9),
                |start, end| {
                    calls.borrow_mut().extend(
                        [1].into_iter()
                            .chain(start.to_array().map(f32::to_bits))
                            .chain(end.to_array().map(f32::to_bits)),
                    );
                    Ok::<_, Infallible>((input[10] != 0).then(|| vector(11)))
                },
                |distance, contact, pivot| {
                    calls.borrow_mut().extend(
                        [2, distance.to_bits()]
                            .into_iter()
                            .chain(contact.to_array().map(f32::to_bits))
                            .chain(pivot.to_array().map(f32::to_bits)),
                    );
                    if input[14] != 0 {
                        calls.borrow_mut().push(3);
                        Ok(Some(float(15)))
                    } else {
                        Ok(None)
                    }
                },
            )?;
            assert_eq!(
                eye.to_array().map(f32::to_bits),
                expected[..3],
                "line {}",
                line_number + 1
            );
            assert_eq!(
                *calls.borrow(),
                groups[2],
                "trace order/arguments line {}",
                line_number + 1
            );
        }
    }
    Ok(())
}

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

/// `$CMA` replaces the principal height with stock's half-duration cosine ease.
#[test]
fn animated_mount_marker_drives_smoothed_camera_height() -> Result<(), Box<dyn Error>> {
    let base = resolve_camera_subject_height(CameraSubjectGeometry::new(None, 0.0, 1.0))?;
    let mut state = PlayerCameraHeightState::new(base);
    state.set_mounted(true, 0.0)?;
    state.update_mount(MountCameraGeometry::new(Some(3.233_333_3), None), 0.0)?;

    assert!((state.sample(0.0)?.subject_height().value() - 0.833_333_3).abs() < 0.000_001);
    let midpoint = state.sample(500.0)?;
    assert_eq!(
        midpoint.subject_height().source(),
        CameraSubjectHeightSource::AnimatedMountMarker
    );
    assert!((midpoint.subject_height().value() - 2.033_333_3).abs() < 0.000_001);
    assert!((state.sample(1_000.0)?.subject_height().value() - 3.233_333_3).abs() < 0.000_001);
    Ok(())
}

/// `$CFM` is a separate authored offset and is not relatched every frame.
#[test]
fn fixed_mount_marker_latches_once_per_mount_generation() -> Result<(), Box<dyn Error>> {
    let base = resolve_camera_subject_height(CameraSubjectGeometry::new(None, 0.0, 1.0))?;
    let mut state = PlayerCameraHeightState::new(base);
    state.set_mounted(true, 0.0)?;
    state.update_mount(MountCameraGeometry::new(None, Some(2.0)), 0.0)?;
    state.update_mount(MountCameraGeometry::new(None, Some(8.0)), 250.0)?;

    let midpoint = state.sample(500.0)?;
    assert_eq!(midpoint.subject_height().source(), base.source());
    assert!((midpoint.subject_height().value() - base.value()).abs() < 0.000_001);
    assert!((midpoint.flying_mount_height() - 1.0).abs() < 0.000_001);
    let settled = state.sample(1_000.0)?;
    assert!((settled.subject_height().value() - base.value()).abs() < 0.000_001);
    assert!((settled.flying_mount_height() - 2.0).abs() < 0.000_001);

    let pose = resolve_mounted_player_camera_pose(
        WorldTransform::new(Vec3::ZERO, 0.0),
        PlayerViewState::STOCK_VIEW_2,
        settled,
    )?;
    let ordinary = resolve_player_camera_pose(
        WorldTransform::new(Vec3::ZERO, 0.0),
        PlayerViewState::STOCK_VIEW_2,
        base,
    )?;
    assert!((pose.orbit_pivot().z - base.value()).abs() < 0.000_001);
    assert!((pose.eye() - ordinary.eye()).abs_diff_eq(ordinary.up() * 2.0, 0.000_001));
    assert!((pose.target() - ordinary.target()).abs_diff_eq(ordinary.up() * 2.0, 0.000_001));
    assert!((pose.flying_mount_height() - 2.0).abs() < 0.000_001);

    state.set_mounted(false, 1_000.0)?;
    let dismounted = state.sample(2_000.0)?;
    assert!((dismounted.subject_height().value() - base.value()).abs() < 0.000_001);
    assert_eq!(dismounted.subject_height().source(), base.source());
    assert!(dismounted.flying_mount_height().abs() < 0.000_001);
    Ok(())
}

/// Stock's 0.05-unit `$CMA` tolerance avoids retargeting on marker jitter.
#[test]
fn animated_mount_marker_tolerance_preserves_existing_target() -> Result<(), Box<dyn Error>> {
    let base = resolve_camera_subject_height(CameraSubjectGeometry::new(None, 0.0, 1.0))?;
    let mut state = PlayerCameraHeightState::new(base);
    state.set_mounted(true, 0.0)?;
    state.update_mount(MountCameraGeometry::new(Some(3.0), None), 0.0)?;
    let settled_at = 0.5 * (3.0 - base.value()) / 1.2 * 1_000.0;
    assert!((state.sample(settled_at)?.subject_height().value() - 3.0).abs() < 0.000_001);

    state.update_mount(MountCameraGeometry::new(Some(3.0 + 0.05), None), settled_at)?;
    assert!((state.sample(settled_at + 1_000.0)?.subject_height().value() - 3.0).abs() < 0.000_001);
    Ok(())
}

/// Invalid mount marker data is rejected without changing the active target.
#[test]
fn invalid_mount_camera_geometry_has_no_fallback() -> Result<(), Box<dyn Error>> {
    let base = resolve_camera_subject_height(CameraSubjectGeometry::new(None, 0.0, 1.0))?;
    let mut state = PlayerCameraHeightState::new(base);
    state.set_mounted(true, 0.0)?;

    assert_eq!(
        state.update_mount(MountCameraGeometry::new(Some(f32::NAN), None), 0.0),
        Err(MountCameraHeightError::NonFiniteAnimatedHeight)
    );
    assert_eq!(
        state.update_mount(MountCameraGeometry::new(None, Some(f32::INFINITY)), 0.0),
        Err(MountCameraHeightError::NonFiniteFixedHeight)
    );
    assert_eq!(
        state.sample(f32::NAN),
        Err(MountCameraHeightError::NonFiniteTime)
    );
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
