//! Original x86 passenger-space matrices, collection boxes, and cache decisions.

use std::error::Error;
use std::str::SplitWhitespace;

use glam::{Mat4, Vec3};
use solarity_systems::{
    MovementCollisionBounds, MovementCollisionTriangle, MovementCollisionVolume, MovementFallMode,
    MovementFallTrajectory, MovementGroundProfile, MovementIntervalMode, MovementIntervalRequest,
    MovementTransportFrame,
};

#[test]
fn passenger_matrices_faces_and_angles_match_native_instructions() -> Result<(), Box<dyn Error>> {
    let mut count = 0;
    for line in records(include_str!("fixtures/passenger-frame-native.txt")) {
        let mut fields = line.split_whitespace();
        let parent = matrix(&mut fields)?;
        let facing = scalar(&mut fields)?;
        let angle = scalar(&mut fields)?;
        let point = vector(&mut fields)?;
        let normal = vector(&mut fields)?;
        let vertices = [
            vector(&mut fields)?,
            vector(&mut fields)?,
            vector(&mut fields)?,
        ];
        let frame = MovementTransportFrame::new(parent, facing)?;
        let expected_reverse = matrix(&mut fields)?;
        assert_eq!(
            frame.local_matrix().to_cols_array().map(f32::to_bits),
            expected_reverse.to_cols_array().map(f32::to_bits),
            "inverse case {count}"
        );
        assert_vector(frame.world_position(point), vector(&mut fields)?, count);
        assert_vector(frame.local_position(point), vector(&mut fields)?, count);
        assert_eq!(
            frame.world_orientation(angle).to_bits(),
            scalar(&mut fields)?.to_bits(),
            "world angle {count}"
        );
        assert_eq!(
            frame.local_orientation(angle).to_bits(),
            scalar(&mut fields)?.to_bits(),
            "local angle {count}"
        );
        let triangle =
            frame.local_triangle(&MovementCollisionTriangle::with_normal(vertices, normal)?)?;
        assert_vector(triangle.normal(), vector(&mut fields)?, count);
        for &point in triangle.vertices() {
            assert_vector(point, vector(&mut fields)?, count);
        }
        assert!(fields.next().is_none());
        count += 1;
    }
    assert_eq!(count, 136);
    Ok(())
}

#[test]
fn passenger_interval_bounds_match_native_world_body_and_local_fall_curve()
-> Result<(), Box<dyn Error>> {
    let mut count = 0;
    for line in records(include_str!("fixtures/passenger-interval-native.txt")) {
        let mut fields = line.split_whitespace();
        let kind = integer(&mut fields)?;
        let player = integer(&mut fields)? != 0;
        let slow = integer(&mut fields)? != 0;
        let duration_ms = integer(&mut fields)?;
        let fall_time_ms = integer(&mut fields)?;
        let frame = MovementTransportFrame::new(matrix(&mut fields)?, 0.)?;
        let position = vector(&mut fields)?;
        let radius = scalar(&mut fields)?;
        let height = scalar(&mut fields)?;
        let distance = scalar(&mut fields)?;
        let direction = vector(&mut fields)?;
        let step_height = scalar(&mut fields)?;
        let launch_height = scalar(&mut fields)?;
        let downward = scalar(&mut fields)?;
        let mode = match kind {
            0 => MovementIntervalMode::Grounded(if player {
                MovementGroundProfile::PlayerControlled { step_height }
            } else {
                MovementGroundProfile::Other
            }),
            1 => MovementIntervalMode::Airborne {
                fall_time_ms,
                launch_height,
                trajectory: MovementFallTrajectory::new(
                    if slow {
                        MovementFallMode::Slow
                    } else {
                        MovementFallMode::Normal
                    },
                    downward,
                )?,
            },
            2 => MovementIntervalMode::SwimmingOrFlying,
            _ => return Err("invalid fixture mode".into()),
        };
        let bounds = MovementIntervalRequest {
            position,
            radius,
            height,
            distance,
            direction,
            duration_ms,
            mode,
        }
        .collection_bounds_in_frame(frame)?;
        for actual in [
            bounds.body().minimum(),
            bounds.body().maximum(),
            bounds.query().minimum(),
            bounds.query().maximum(),
        ] {
            assert_vector(actual, vector(&mut fields)?, count);
        }
        assert!(fields.next().is_none());
        count += 1;
    }
    assert_eq!(count, 192);
    Ok(())
}

#[test]
fn passenger_sweep_cache_matches_native_hits_and_world_axis_refreshes() -> Result<(), Box<dyn Error>>
{
    let mut count = 0;
    for line in records(include_str!("fixtures/passenger-cache-native.txt")) {
        let mut fields = line.split_whitespace();
        let frame = MovementTransportFrame::new(matrix(&mut fields)?, 0.)?;
        let position = vector(&mut fields)?;
        let radius = scalar(&mut fields)?;
        let height = scalar(&mut fields)?;
        let direction = vector(&mut fields)?;
        let distance = scalar(&mut fields)?;
        let cached = MovementCollisionBounds::new(vector(&mut fields)?, vector(&mut fields)?)?;
        let expected_miss = integer(&mut fields)? != 0;
        let result = MovementCollisionVolume::new(position, radius, height)?
            .sweep_refresh_bounds_in_frame(direction, distance, cached, frame)?;
        assert_eq!(result.is_some(), expected_miss, "cache decision {count}");
        let actual = result.unwrap_or(cached);
        assert_vector(actual.minimum(), vector(&mut fields)?, count);
        assert_vector(actual.maximum(), vector(&mut fields)?, count);
        assert!(fields.next().is_none());
        count += 1;
    }
    assert_eq!(count, 480);
    Ok(())
}

#[test]
fn passenger_frame_rejects_invalid_inputs_and_preserves_native_scaled_transpose()
-> Result<(), Box<dyn Error>> {
    assert!(MovementTransportFrame::new(Mat4::ZERO, 0.).is_err());
    assert!(MovementTransportFrame::new(Mat4::IDENTITY, f32::NAN).is_err());
    let frame = MovementTransportFrame::new(Mat4::from_scale(Vec3::splat(2.)), 0.)?;
    assert_eq!(frame.local_position(Vec3::ONE), Vec3::splat(2.));
    let triangle = MovementCollisionTriangle::with_normal([Vec3::splat(f32::MAX); 3], Vec3::Z)?;
    assert!(frame.local_triangle(&triangle).is_err());
    Ok(())
}

fn records(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
}

fn scalar(fields: &mut SplitWhitespace<'_>) -> Result<f32, Box<dyn Error>> {
    Ok(f32::from_bits(u32::from_str_radix(
        fields.next().ok_or("missing scalar")?,
        16,
    )?))
}

fn integer(fields: &mut SplitWhitespace<'_>) -> Result<u32, Box<dyn Error>> {
    Ok(fields.next().ok_or("missing integer")?.parse()?)
}

fn vector(fields: &mut SplitWhitespace<'_>) -> Result<Vec3, Box<dyn Error>> {
    Ok(Vec3::new(scalar(fields)?, scalar(fields)?, scalar(fields)?))
}

fn matrix(fields: &mut SplitWhitespace<'_>) -> Result<Mat4, Box<dyn Error>> {
    let mut values = [0.; 16];
    for value in &mut values {
        *value = scalar(fields)?;
    }
    Ok(Mat4::from_cols_array(&values))
}

fn assert_vector(actual: Vec3, expected: Vec3, case: usize) {
    assert_eq!(
        actual.to_array().map(f32::to_bits),
        expected.to_array().map(f32::to_bits),
        "case {case}: {actual:?} != {expected:?}"
    );
}
