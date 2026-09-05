//! Independent original-x86 ground, step, and fall-transition captures.

use glam::{Vec2, Vec3};
use solarity_systems::{
    MovementCollisionTriangle, MovementFallAdmission, MovementFallMode, MovementGroundContinuation,
    MovementGroundInterval, MovementGroundProfile, MovementGroundSnapshot, MovementGroundState,
};
use std::{error::Error, str::SplitWhitespace};

#[test]
fn ground_intervals_match_original_x86_state() -> Result<(), Box<dyn Error>> {
    for line in include_str!("fixtures/movement-ground-advance-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let mut f = line.split_whitespace();
        let name = f.next().ok_or("missing name")?;
        let position = vector(&mut f)?;
        let radius = scalar(&mut f)?;
        let height = scalar(&mut f)?;
        let duration_ms = integer(&mut f)?;
        let distance = scalar(&mut f)?;
        let heading = Vec2::new(scalar(&mut f)?, scalar(&mut f)?);
        let player = integer(&mut f)? != 0;
        let step_height = scalar(&mut f)?;
        let step = integer(&mut f)? != 0;
        let blocked = integer(&mut f)? != 0;
        let slow = integer(&mut f)? != 0;
        let horizontal_direction = Vec2::new(scalar(&mut f)?, scalar(&mut f)?);
        let direction = vector(&mut f)?;
        let horizontal_speed = scalar(&mut f)?;
        let initial_step_anchor = scalar(&mut f)?;
        let initial_fall_ms = integer(&mut f)?;
        let initial_launch_height = scalar(&mut f)?;
        let initial_launch = scalar(&mut f)?;
        let count = integer(&mut f)?;
        let triangles = (0..count)
            .map(|_| {
                Ok(MovementCollisionTriangle::new([
                    vector(&mut f)?,
                    vector(&mut f)?,
                    vector(&mut f)?,
                ])?)
            })
            .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
        let expected_consumed = integer(&mut f)?;
        let expected_position = vector(&mut f)?;
        let expected_clock = integer(&mut f)?;
        let falling = integer(&mut f)? != 0;
        let expected_step = integer(&mut f)? != 0;
        let expected_anchor = scalar(&mut f)?;
        let reanchor = integer(&mut f)? != 0;
        let contact: i32 = f.next().ok_or("missing contact")?.parse()?;
        let launch_height = scalar(&mut f)?;
        let launch = scalar(&mut f)?;
        let expected_horizontal = Vec2::new(scalar(&mut f)?, scalar(&mut f)?);
        let expected_direction = vector(&mut f)?;
        let expected_speed = scalar(&mut f)?;
        assert!(f.next().is_none(), "{name}: trailing fields");
        let result = MovementGroundState::new(MovementGroundSnapshot {
            position,
            step_anchor: step.then_some(initial_step_anchor),
            fall_time_ms: initial_fall_ms,
            launch_height: initial_launch_height,
            initial_downward_speed: initial_launch,
            horizontal_direction,
            horizontal_speed,
            direction,
            mode: if slow {
                MovementFallMode::Slow
            } else {
                MovementFallMode::Normal
            },
            fall_admission: if blocked {
                MovementFallAdmission::Suppressed
            } else {
                MovementFallAdmission::Allowed
            },
        })?
        .advance(
            MovementGroundInterval {
                duration_ms,
                distance,
                direction: heading,
                radius,
                height,
                profile: if player {
                    MovementGroundProfile::PlayerControlled { step_height }
                } else {
                    MovementGroundProfile::Other
                },
            },
            &triangles,
        )
        .map_err(|err| format!("{name}: {err}"))?;
        assert_eq!(result.consumed_ms, expected_consumed, "{name}: consumed");
        assert_eq!(result.reset_motion_anchor, reanchor, "{name}: reanchor");
        assert_eq!(
            result.contact_triangle,
            usize::try_from(contact).ok(),
            "{name}: contact"
        );
        let (position, clock, height, speed, horizontal, basis, velocity) =
            match result.continuation {
                MovementGroundContinuation::Grounded(state) => {
                    assert!(!falling, "{name}: expected falling");
                    let s = state.snapshot();
                    assert_eq!(s.step_anchor.is_some(), expected_step, "{name}: step");
                    if let Some(anchor) = s.step_anchor {
                        close(name, "step anchor", anchor, expected_anchor);
                    }
                    (
                        s.position,
                        s.fall_time_ms,
                        s.launch_height,
                        s.initial_downward_speed,
                        s.horizontal_direction,
                        s.direction,
                        s.horizontal_speed,
                    )
                }
                MovementGroundContinuation::Falling(state) => {
                    assert!(falling, "{name}: unexpected falling");
                    assert!(!expected_step, "{name}: falling retained step");
                    let s = state.snapshot();
                    (
                        s.position,
                        s.fall_time_ms,
                        s.launch_height,
                        s.initial_downward_speed,
                        s.horizontal_direction,
                        s.direction,
                        s.horizontal_speed,
                    )
                }
            };
        assert!(
            (position - expected_position).length() < 0.0001,
            "{name}: position {position} expected {expected_position}"
        );
        assert_eq!(clock, expected_clock, "{name}: fall clock");
        close(name, "launch height", height, launch_height);
        close(name, "launch speed", speed, launch);
        assert!(
            (horizontal - expected_horizontal).length() < 0.0001,
            "{name}: horizontal basis"
        );
        assert!(
            (basis - expected_direction).length() < 0.0001,
            "{name}: full basis"
        );
        close(name, "speed", velocity, expected_speed);
    }
    Ok(())
}

/// Invalid owner fields must not bypass collision-loop comparisons.
#[test]
fn ground_intervals_reject_invalid_state_and_geometry() -> Result<(), Box<dyn Error>> {
    let snapshot = MovementGroundSnapshot {
        position: Vec3::ZERO,
        step_anchor: None,
        fall_time_ms: 73,
        launch_height: 101.0,
        initial_downward_speed: -7.95,
        horizontal_direction: Vec2::X,
        horizontal_speed: 7.0,
        direction: Vec3::X,
        mode: MovementFallMode::Normal,
        fall_admission: MovementFallAdmission::Allowed,
    };
    for invalid in [
        MovementGroundSnapshot {
            position: Vec3::NAN,
            ..snapshot
        },
        MovementGroundSnapshot {
            step_anchor: Some(f32::NAN),
            ..snapshot
        },
        MovementGroundSnapshot {
            horizontal_direction: Vec2::NAN,
            ..snapshot
        },
        MovementGroundSnapshot {
            horizontal_speed: -1.0,
            ..snapshot
        },
        MovementGroundSnapshot {
            launch_height: f32::INFINITY,
            ..snapshot
        },
    ] {
        assert!(matches!(
            MovementGroundState::new(invalid),
            Err(solarity_systems::MovementGroundAdvanceError::InvalidState)
        ));
    }
    let state = MovementGroundState::new(snapshot)?;
    let interval = MovementGroundInterval {
        duration_ms: 16,
        distance: 0.112,
        direction: Vec2::X,
        radius: 0.5,
        height: 2.0,
        profile: MovementGroundProfile::PlayerControlled { step_height: 1.0 },
    };
    for invalid in [
        MovementGroundInterval {
            distance: f32::NAN,
            ..interval
        },
        MovementGroundInterval {
            distance: -1.0,
            ..interval
        },
        MovementGroundInterval {
            direction: Vec2::NAN,
            ..interval
        },
        MovementGroundInterval {
            profile: MovementGroundProfile::PlayerControlled { step_height: -1.0 },
            ..interval
        },
    ] {
        assert!(matches!(
            state.advance(invalid, &[]),
            Err(solarity_systems::MovementGroundAdvanceError::InvalidState)
        ));
    }
    assert!(matches!(
        state.advance(
            MovementGroundInterval {
                radius: 0.0,
                ..interval
            },
            &[]
        ),
        Err(solarity_systems::MovementGroundAdvanceError::Collision(
            solarity_systems::MovementSweepError::InvalidVolume
        ))
    ));
    Ok(())
}

fn close(name: &str, field: &str, actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.0001,
        "{name}: {field} {actual} expected {expected}"
    );
}
fn scalar(f: &mut SplitWhitespace<'_>) -> Result<f32, Box<dyn Error>> {
    Ok(f32::from_bits(u32::from_str_radix(
        f.next().ok_or("missing float")?,
        16,
    )?))
}
fn integer(f: &mut SplitWhitespace<'_>) -> Result<u32, Box<dyn Error>> {
    Ok(f.next().ok_or("missing integer")?.parse()?)
}
fn vector(f: &mut SplitWhitespace<'_>) -> Result<Vec3, Box<dyn Error>> {
    Ok(Vec3::new(scalar(f)?, scalar(f)?, scalar(f)?))
}
