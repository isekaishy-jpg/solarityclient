//! Complete native fall intervals, including repeated collision/state updates.

use std::{error::Error, str::SplitWhitespace};

#[path = "support/movement_geometry.rs"]
mod geometry;

use glam::{Vec2, Vec3};
use solarity_systems::{
    MovementCollisionTriangle, MovementFallAdvanceError, MovementFallAdvancePolicy,
    MovementFallContactError, MovementFallContinuation, MovementFallInterval, MovementFallMode,
    MovementFallPhase, MovementFallSnapshot, MovementFallState, MovementSupportProfile,
    MovementSweepError,
};

/// Original x86 execution is independent of the interval implementation.
#[test]
fn fall_intervals_match_original_x86_state() -> Result<(), Box<dyn Error>> {
    for line in [
        include_str!("fixtures/movement-fall-advance-native.txt"),
        include_str!("fixtures/movement-fall-geometry-native.txt"),
    ]
    .into_iter()
    .flat_map(str::lines)
    {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let mut fields = line.split_whitespace();
        let name = fields.next().ok_or("missing name")?;
        let (failure, deferred) = if name.starts_with("fault:") {
            (integer(&mut fields)?, integer(&mut fields)?)
        } else {
            (0, 0)
        };
        let position = vector(&mut fields)?;
        let radius = scalar(&mut fields)?;
        let height = scalar(&mut fields)?;
        let displacement = vector(&mut fields)?;
        let initial_downward_speed = scalar(&mut fields)?;
        let fall_time_ms = integer(&mut fields)?;
        let duration_ms = integer(&mut fields)?;
        let horizontal_speed = scalar(&mut fields)?;
        let horizontal_direction = Vec2::new(scalar(&mut fields)?, scalar(&mut fields)?);
        let direction = vector(&mut fields)?;
        let player = integer(&mut fields)? != 0;
        let slow = integer(&mut fields)? != 0;
        let far = integer(&mut fields)? != 0;
        let live = integer(&mut fields)? != 0;
        let moving = integer(&mut fields)? != 0;
        let launch_height = scalar(&mut fields)?;
        let count = integer(&mut fields)?;
        let triangles = (0..count)
            .map(|_| {
                Ok(MovementCollisionTriangle::new([
                    vector(&mut fields)?,
                    vector(&mut fields)?,
                    vector(&mut fields)?,
                ])?)
            })
            .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
        let expected_consumed = integer(&mut fields)?;
        let expected_position = vector(&mut fields)?;
        let expected_fall_time = integer(&mut fields)?;
        let expected_falling = integer(&mut fields)? != 0;
        let expected_far = integer(&mut fields)? != 0;
        let expected_reanchor = integer(&mut fields)? != 0;
        let expected_launch_height = scalar(&mut fields)?;
        let expected_launch = scalar(&mut fields)?;
        let expected_horizontal = Vec2::new(scalar(&mut fields)?, scalar(&mut fields)?);
        let expected_direction = vector(&mut fields)?;
        let expected_speed = scalar(&mut fields)?;
        let expected_notify = integer(&mut fields)? != 0;
        let expected_contact: i32 = fields.next().ok_or("missing contact")?.parse()?;
        assert!(fields.next().is_none(), "{name}: trailing fields");
        let state = MovementFallState::new(MovementFallSnapshot {
            position,
            fall_time_ms,
            launch_height,
            initial_downward_speed,
            horizontal_direction,
            horizontal_speed,
            direction,
            mode: if slow {
                MovementFallMode::Slow
            } else {
                MovementFallMode::Normal
            },
            phase: if far {
                MovementFallPhase::FallingFar
            } else {
                MovementFallPhase::Falling
            },
        })?;
        let interval = MovementFallInterval {
            duration_ms,
            displacement,
            radius,
            height,
            support_profile: if player {
                MovementSupportProfile::PlayerControlled
            } else {
                MovementSupportProfile::Other
            },
            policy: if !live {
                MovementFallAdvancePolicy::Trial
            } else if moving {
                MovementFallAdvancePolicy::LiveTranslating
            } else {
                MovementFallAdvancePolicy::Live
            },
        };
        let result = if name.starts_with("fault:") {
            state.advance_with_geometry(interval, &mut geometry::Geometry::new(&triangles, failure))
        } else {
            state.advance(interval, &triangles)
        }?;
        assert_eq!(
            result.geometry_unavailable,
            failure != 0,
            "{name}: provider failure"
        );
        assert_eq!(result.skipped_time_ms, deferred, "{name}: deferred clock");
        assert_eq!(
            result.consumed_ms, expected_consumed,
            "{name}: consumed time"
        );
        assert_eq!(
            result.reset_motion_anchor, expected_reanchor,
            "{name}: anchor"
        );
        assert_eq!(
            result.notify_ceiling_reset, expected_notify,
            "{name}: notification"
        );
        assert_eq!(
            result.contact_triangle,
            usize::try_from(expected_contact).ok(),
            "{name}: contact identity"
        );
        let (position, fall_time) = match result.continuation {
            MovementFallContinuation::Airborne(state) => {
                assert!(expected_falling, "{name}: expected landing");
                let snapshot = state.snapshot();
                assert_eq!(
                    snapshot.phase == MovementFallPhase::FallingFar,
                    expected_far,
                    "{name}: falling far"
                );
                assert!(
                    (snapshot.launch_height - expected_launch_height).abs() < 0.0001,
                    "{name}: launch height"
                );
                assert_eq!(
                    snapshot.initial_downward_speed.to_bits(),
                    expected_launch.to_bits(),
                    "{name}: launch speed"
                );
                assert!(
                    (snapshot.horizontal_direction - expected_horizontal).length() < 0.0001,
                    "{name}: horizontal direction {:?} expected {expected_horizontal}",
                    snapshot.horizontal_direction
                );
                assert!(
                    (snapshot.direction - expected_direction).length() < 0.0001,
                    "{name}: movement basis"
                );
                assert!(
                    (snapshot.horizontal_speed - expected_speed).abs() < 0.0001,
                    "{name}: horizontal speed {} expected {expected_speed}",
                    snapshot.horizontal_speed
                );
                (snapshot.position, snapshot.fall_time_ms)
            }
            MovementFallContinuation::Landed {
                position,
                fall_time_ms,
            } => {
                assert!(!expected_falling, "{name}: unexpected landing");
                (position, fall_time_ms)
            }
        };
        assert!(
            (position - expected_position).length() < 0.0001,
            "{name}: position {position} expected {expected_position}"
        );
        assert_eq!(fall_time, expected_fall_time, "{name}: fall clock");
    }
    Ok(())
}

/// Poisoned state or displacement must not bypass the loop's comparisons.
#[test]
fn fall_intervals_reject_nonfinite_state_and_geometry() -> Result<(), Box<dyn Error>> {
    let snapshot = MovementFallSnapshot {
        position: Vec3::ZERO,
        fall_time_ms: 0,
        launch_height: 0.0,
        initial_downward_speed: -7.95,
        horizontal_direction: Vec2::X,
        horizontal_speed: 7.0,
        direction: Vec3::X,
        mode: MovementFallMode::Normal,
        phase: MovementFallPhase::Falling,
    };
    for invalid in [
        MovementFallSnapshot {
            position: Vec3::NAN,
            ..snapshot
        },
        MovementFallSnapshot {
            horizontal_speed: -1.0,
            ..snapshot
        },
        MovementFallSnapshot {
            initial_downward_speed: f32::INFINITY,
            ..snapshot
        },
        MovementFallSnapshot {
            direction: Vec3::NAN,
            ..snapshot
        },
    ] {
        assert!(matches!(
            MovementFallState::new(invalid),
            Err(MovementFallAdvanceError::InvalidState)
        ));
    }
    let state = MovementFallState::new(snapshot)?;
    let interval = MovementFallInterval {
        duration_ms: 16,
        displacement: Vec3::NAN,
        radius: 0.5,
        height: 2.0,
        support_profile: MovementSupportProfile::PlayerControlled,
        policy: MovementFallAdvancePolicy::Live,
    };
    assert!(matches!(
        state.advance(interval, &[]),
        Err(MovementFallAdvanceError::Contact(
            MovementFallContactError::Sweep(MovementSweepError::InvalidDisplacement)
        ))
    ));
    assert!(matches!(
        state.advance(
            MovementFallInterval {
                radius: 0.0,
                displacement: Vec3::ZERO,
                ..interval
            },
            &[]
        ),
        Err(MovementFallAdvanceError::Contact(
            MovementFallContactError::Sweep(MovementSweepError::InvalidVolume)
        ))
    ));
    Ok(())
}

/// Decodes a native float image without decimal conversion.
fn scalar(fields: &mut SplitWhitespace<'_>) -> Result<f32, Box<dyn Error>> {
    Ok(f32::from_bits(u32::from_str_radix(
        fields.next().ok_or("missing float")?,
        16,
    )?))
}

/// Reads unsigned clocks, counts, and boolean fixture fields.
fn integer(fields: &mut SplitWhitespace<'_>) -> Result<u32, Box<dyn Error>> {
    Ok(fields.next().ok_or("missing integer")?.parse()?)
}

/// Reads XYZ in the admitted collision coordinate space.
fn vector(fields: &mut SplitWhitespace<'_>) -> Result<Vec3, Box<dyn Error>> {
    Ok(Vec3::new(scalar(fields)?, scalar(fields)?, scalar(fields)?))
}
