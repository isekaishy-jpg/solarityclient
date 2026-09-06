//! Original-x86 expanded collision collection boxes, independent of Rust math.

use glam::Vec3;
use solarity_systems::{
    MovementFallMode, MovementFallTrajectory, MovementGroundProfile, MovementIntervalMode,
    MovementIntervalRequest,
};
use std::{error::Error, str::SplitWhitespace};

#[test]
fn interval_collection_bounds_match_original_instructions() -> Result<(), Box<dyn Error>> {
    let mut count = 0;
    for (index, line) in include_str!("fixtures/movement-interval-bounds-native.txt")
        .lines()
        .enumerate()
    {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let mut fields = line.split_whitespace();
        let mode = integer(&mut fields)?;
        let player = integer(&mut fields)? != 0;
        let slow = integer(&mut fields)? != 0;
        let duration_ms = integer(&mut fields)?;
        let fall_time_ms = integer(&mut fields)?;
        let position = vector(&mut fields)?;
        let radius = scalar(&mut fields)?;
        let height = scalar(&mut fields)?;
        let distance = scalar(&mut fields)?;
        let direction = vector(&mut fields)?;
        let step_height = scalar(&mut fields)?;
        let launch_height = scalar(&mut fields)?;
        let downward_speed = scalar(&mut fields)?;
        let mode = match mode {
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
                    downward_speed,
                )?,
            },
            2 => MovementIntervalMode::SwimmingOrFlying,
            _ => return Err("unknown mode".into()),
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
        .collection_bounds()?;
        for actual in [
            bounds.body().minimum(),
            bounds.body().maximum(),
            bounds.query().minimum(),
            bounds.query().maximum(),
        ] {
            let expected = vector(&mut fields)?;
            assert_eq!(
                actual.to_array().map(f32::to_bits),
                expected.to_array().map(f32::to_bits),
                "line {} mode {mode:?}: {actual:?} != {expected:?}",
                index + 1
            );
        }
        assert!(fields.next().is_none());
        count += 1;
    }
    assert_eq!(count, 1408);
    Ok(())
}

#[test]
fn interval_bounds_reject_invalid_inputs_and_keep_stationary_step_probes()
-> Result<(), Box<dyn Error>> {
    let request = MovementIntervalRequest {
        position: Vec3::ZERO,
        radius: 0.5,
        height: 2.,
        distance: 0.,
        direction: Vec3::X,
        duration_ms: 0,
        mode: MovementIntervalMode::Grounded(MovementGroundProfile::PlayerControlled {
            step_height: 1.,
        }),
    };
    let bounds = request.collection_bounds()?;
    assert!(bounds.query().minimum().z < -1.);
    assert!(bounds.query().maximum().z > 4.);
    assert!(bounds.query().maximum().x > 1.5);
    for invalid in [
        MovementIntervalRequest {
            position: Vec3::NAN,
            ..request
        },
        MovementIntervalRequest {
            direction: Vec3::NAN,
            ..request
        },
        MovementIntervalRequest {
            distance: -1.,
            ..request
        },
        MovementIntervalRequest {
            radius: -1.,
            ..request
        },
        MovementIntervalRequest {
            height: f32::INFINITY,
            ..request
        },
        MovementIntervalRequest {
            distance: f32::MAX,
            direction: Vec3::splat(f32::MAX),
            ..request
        },
        MovementIntervalRequest {
            mode: MovementIntervalMode::Grounded(MovementGroundProfile::PlayerControlled {
                step_height: -1.,
            }),
            ..request
        },
        MovementIntervalRequest {
            mode: MovementIntervalMode::Airborne {
                fall_time_ms: 0,
                launch_height: f32::NAN,
                trajectory: MovementFallTrajectory::new(MovementFallMode::Normal, 0.)?,
            },
            ..request
        },
    ] {
        assert!(invalid.collection_bounds().is_err(), "{invalid:?}");
    }
    Ok(())
}

fn integer(fields: &mut SplitWhitespace<'_>) -> Result<u32, Box<dyn Error>> {
    Ok(fields.next().ok_or("missing integer")?.parse()?)
}
fn scalar(fields: &mut SplitWhitespace<'_>) -> Result<f32, Box<dyn Error>> {
    Ok(f32::from_bits(u32::from_str_radix(
        fields.next().ok_or("missing scalar")?,
        16,
    )?))
}
fn vector(fields: &mut SplitWhitespace<'_>) -> Result<Vec3, Box<dyn Error>> {
    Ok(Vec3::new(scalar(fields)?, scalar(fields)?, scalar(fields)?))
}
