//! Original executable fixtures exercise clocks through the public path owner.

use glam::Vec3;
use solarity_ecs::WorldTransform;
use solarity_systems::{MovementSpline, MovementSplineDefinition, MovementSplineFacing};

/// Fixed controls and scalar inputs written into the original native owner.
fn definition() -> MovementSplineDefinition {
    MovementSplineDefinition {
        flags: 0,
        facing: MovementSplineFacing::Direction,
        id: 37,
        elapsed_ms: 300,
        duration_ms: 2000,
        duration_scale: 1.0,
        next_duration_scale: 1.5,
        vertical_acceleration: 4.0,
        effect_start_ms: 250,
        nodes: vec![
            Vec3::new(-5.0, 2.0, 30.0),
            Vec3::new(0.0, 0.0, 30.0),
            Vec3::new(5.0, -3.0, 27.0),
            Vec3::new(10.0, 2.0, 25.0),
        ],
        destination: Vec3::new(5.0, -3.0, 27.0),
    }
}

/// Compare complete clock/transform behavior, allowing the geometry fixture's
/// established float tolerance. Unit_C completion adds 0x400 after 0x100.
#[test]
fn clock_and_transforms_match_original_evaluations() -> Result<(), Box<dyn std::error::Error>> {
    for (index, line) in include_str!("fixtures/movement-spline-native.txt")
        .lines()
        .enumerate()
    {
        if line.starts_with('#') {
            continue;
        }
        let row = line
            .split_whitespace()
            .map(|word| u32::from_str_radix(word, 16))
            .collect::<Result<Vec<_>, _>>()?;
        let mut source = definition();
        source.flags = row[0];
        source.duration_ms = row[1];
        source.duration_scale = f32::from_bits(row[2]);
        source.next_duration_scale = f32::from_bits(row[3]);
        source.elapsed_ms = row[5];
        let current = WorldTransform::new(Vec3::new(0.0, 0.0, 30.0), 0.7);
        let mut spline = MovementSpline::new(source, row[4], current)?;
        let transform = spline.advance(row[6], 1, current, |_| None)?;
        let complete = row[7] & 0x100 != 0;
        assert_eq!(
            spline.motion().flags,
            row[7] | if complete { 0x400 } else { 0 },
            "row {index}"
        );
        let actual = [
            transform.position().x,
            transform.position().y,
            transform.position().z,
            transform.orientation(),
        ];
        for (component, value) in actual.into_iter().enumerate() {
            // 006EB0B0 places the separate destination after 0098CA00 requests
            // completion, including a parabolic effect with duration scaling.
            let expected = if complete && component < 3 {
                [5.0, -3.0, 27.0][component]
            } else {
                f32::from_bits(row[12 + component])
            };
            assert!(
                (value - expected).abs() <= 0.00001_f32.max(expected.abs() * 0.000001),
                "row {index} component {component}: {value} != {expected}"
            );
        }
    }
    Ok(())
}

/// Completion resolves the target at the endpoint and stays there on later ticks.
#[test]
fn endpoint_facing_is_applied_once() -> Result<(), Box<dyn std::error::Error>> {
    let mut source = definition();
    source.facing = MovementSplineFacing::Target(99);
    let current = WorldTransform::new(Vec3::ZERO, 0.0);
    let mut spline = MovementSpline::new(source, 1000, current)?;
    let completed = spline.advance(3000, 1, current, |guid| {
        assert_eq!(guid, 99);
        Some(Vec3::new(5.0, 10.0, 27.0))
    })?;
    assert_eq!(completed.position(), Vec3::new(5.0, -3.0, 27.0));
    assert_eq!(completed.orientation(), std::f32::consts::FRAC_PI_2);
    let retained = spline.advance(6000, 1, completed, |_| None)?;
    assert_eq!(retained, completed);
    Ok(())
}

/// `0098C940` rebuilds the first cycle and resets the replacement owner's
/// elapsed time and scales, even when the frame overshoots the boundary.
#[test]
fn first_cycle_rebuilds_controls_and_resets_the_effect_clock()
-> Result<(), Box<dyn std::error::Error>> {
    let mut source = definition();
    source.flags = 0x180000;
    source.elapsed_ms = 0;
    source.nodes = (0..6).map(|x| Vec3::new(x as f32, 0.0, 0.0)).collect();
    let current = WorldTransform::new(Vec3::ZERO, 0.0);
    let mut spline = MovementSpline::new(source, 1000, current)?;
    let boundary = spline.advance(3500, 1, current, |_| None)?;
    assert_eq!(boundary.position(), Vec3::new(2.0, 0.0, 0.0));
    assert_eq!(spline.motion().flags, 0x80000);
    assert_eq!(spline.motion().length, 2.0);
    let next = spline.advance(4000, 1, boundary, |_| None)?;
    assert_eq!(next.position(), Vec3::new(3.0, 0.0, 0.0));
    Ok(())
}
