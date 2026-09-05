//! Analytic fall curves compared with execution of the original x86 routines.

use std::error::Error;

use solarity_systems::{
    MovementFallCrossing, MovementFallError, MovementFallMode, MovementFallTrajectory,
};

/// The golden scalar images were produced without calling the Rust code.
#[test]
fn fall_distance_and_contact_time_match_native_curves() -> Result<(), Box<dyn Error>> {
    for (index, line) in include_str!("fixtures/movement-fall-native.txt")
        .lines()
        .enumerate()
    {
        if line.starts_with('#') {
            continue;
        }
        let fields: Vec<_> = line.split_whitespace().collect();
        assert_eq!(fields.len(), 6);
        let mode = match fields[1] {
            "0" => MovementFallMode::Normal,
            "1" => MovementFallMode::Slow,
            _ => return Err("invalid fall mode".into()),
        };
        let crossing = match fields[3] {
            "0" => MovementFallCrossing::Descending,
            "1" => MovementFallCrossing::Ascending,
            _ => return Err("invalid crossing".into()),
        };
        let trajectory = MovementFallTrajectory::new(mode, scalar(fields[2])?)?;
        let input = scalar(fields[4])?;
        let expected = scalar(fields[5])?;
        let actual = match fields[0] {
            "distance" => trajectory.distance_at_seconds(input)?,
            "milliseconds" => trajectory.distance_at_millis(u32::from_str_radix(fields[4], 16)?)?,
            "time" => trajectory.seconds_at_distance(input, crossing)?,
            _ => return Err("invalid operation".into()),
        };
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "fixture line {}: {line}; got {actual}, expected {expected}",
            index + 1
        );
    }
    Ok(())
}

/// Signed launch, terminal cap, and roots have useful behavior at the public API.
#[test]
fn jump_height_has_two_crossings_and_terminal_travel_is_linear() -> Result<(), Box<dyn Error>> {
    let jump = MovementFallTrajectory::new(MovementFallMode::Normal, -7.95)?;
    let ascending = jump.seconds_at_distance(-1.0, MovementFallCrossing::Ascending)?;
    let descending = jump.seconds_at_distance(-1.0, MovementFallCrossing::Descending)?;
    assert!(0.0 < ascending && ascending < descending);
    assert!((jump.distance_at_seconds(ascending)? + 1.0).abs() < 0.00001);
    assert!((jump.distance_at_seconds(descending)? + 1.0).abs() < 0.00001);
    let slow = MovementFallTrajectory::new(MovementFallMode::Slow, 100.0)?;
    assert_eq!(slow.distance_at_seconds(2.0)?, 14.0);
    assert_eq!(slow.distance_at_seconds(5.0)?, 35.0);
    Ok(())
}

/// Invalid floats cannot contaminate later collision clocks and coordinates.
#[test]
fn fall_queries_reject_invalid_inputs() -> Result<(), Box<dyn Error>> {
    assert!(matches!(
        MovementFallTrajectory::new(MovementFallMode::Normal, f32::NAN),
        Err(MovementFallError::NonFiniteLaunchSpeed)
    ));
    let trajectory = MovementFallTrajectory::new(MovementFallMode::Normal, 0.0)?;
    for time in [-1.0, f32::NAN, f32::INFINITY] {
        assert_eq!(
            trajectory.distance_at_seconds(time),
            Err(MovementFallError::InvalidElapsedTime)
        );
    }
    assert_eq!(
        trajectory.distance_at_seconds(f32::MAX),
        Err(MovementFallError::NonFiniteResult)
    );
    assert_eq!(
        trajectory.seconds_at_distance(f32::INFINITY, MovementFallCrossing::Descending),
        Err(MovementFallError::NonFiniteDistance)
    );
    Ok(())
}

/// Decodes the captured native float without decimal conversion drift.
fn scalar(value: &str) -> Result<f32, Box<dyn Error>> {
    Ok(f32::from_bits(u32::from_str_radix(value, 16)?))
}
