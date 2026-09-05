//! Original x86 fall response, including edge selection and contact-time roots.

use std::{error::Error, str::SplitWhitespace};

use glam::{Vec2, Vec3};
use solarity_systems::{
    MovementCollisionTriangle, MovementCollisionVolume, MovementFallContactError,
    MovementFallContactKind, MovementFallContactQuery, MovementFallMode, MovementFallTrajectory,
    MovementSupportProfile,
};

/// Compares full contact outcomes with unmodified native geometry/time calls.
#[test]
fn fall_contacts_match_original_x86_response() -> Result<(), Box<dyn Error>> {
    for line in include_str!("fixtures/movement-fall-contact-native.txt").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let mut fields = line.split_whitespace();
        let name = fields.next().ok_or("missing name")?;
        let origin = vector(&mut fields)?;
        let radius = scalar(&mut fields)?;
        let height = scalar(&mut fields)?;
        let delta = vector(&mut fields)?;
        let launch = scalar(&mut fields)?;
        let elapsed_seconds = scalar(&mut fields)?;
        let interval_seconds = scalar(&mut fields)?;
        let horizontal_speed = scalar(&mut fields)?;
        let profile = fields.next().ok_or("missing profile")?;
        let mode = fields.next().ok_or("missing mode")?;
        let launch_height = scalar(&mut fields)?;
        let count: usize = fields.next().ok_or("missing triangle count")?.parse()?;
        let triangles = (0..count)
            .map(|_| {
                Ok(MovementCollisionTriangle::new([
                    vector(&mut fields)?,
                    vector(&mut fields)?,
                    vector(&mut fields)?,
                ])?)
            })
            .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
        let contact = fields.next().ok_or("missing contact")?;
        let land = fields.next().ok_or("missing landing")?;
        let ceiling = fields.next().ok_or("missing ceiling")?;
        let expected_kind = if contact == "0" {
            MovementFallContactKind::Clear
        } else if land == "1" {
            MovementFallContactKind::Land
        } else if ceiling == "1" {
            MovementFallContactKind::Ceiling
        } else {
            MovementFallContactKind::Slide
        };
        let expected_delta = vector(&mut fields)?;
        let expected_distance = scalar(&mut fields)?;
        let expected_correction = Vec2::new(scalar(&mut fields)?, scalar(&mut fields)?);
        let expected_seconds = scalar(&mut fields)?;
        assert!(fields.next().is_none(), "{name}");
        let query = MovementFallContactQuery {
            trajectory: MovementFallTrajectory::new(
                if mode == "0" {
                    MovementFallMode::Normal
                } else {
                    MovementFallMode::Slow
                },
                launch,
            )?,
            launch_height,
            elapsed_seconds,
            interval_seconds,
            horizontal_speed,
            support_profile: if profile == "1" {
                MovementSupportProfile::PlayerControlled
            } else {
                MovementSupportProfile::Other
            },
        };
        let result = query.resolve(
            &MovementCollisionVolume::new(origin, radius, height)?,
            delta,
            &triangles,
        )?;
        assert_eq!(result.kind, expected_kind, "{name}");
        assert!(
            (result.displacement - expected_delta).length() < 0.0001,
            "{name}: delta {:?}, expected {expected_delta}",
            result.displacement
        );
        assert!(
            (result.distance - expected_distance).abs() < 0.0001,
            "{name}: distance {}, expected {expected_distance}",
            result.distance
        );
        assert!(
            (result.horizontal_correction - expected_correction).length() < 0.0001,
            "{name}: correction {:?}, expected {expected_correction}",
            result.horizontal_correction
        );
        assert!(
            (result.consumed_seconds - expected_seconds).abs() < 0.000001,
            "{name}: seconds {}, expected {expected_seconds}",
            result.consumed_seconds
        );
    }
    Ok(())
}

/// Admission errors and the native clear-travel exception for tiny deltas.
#[test]
fn fall_contact_admission_and_clear_travel() -> Result<(), Box<dyn Error>> {
    let volume = MovementCollisionVolume::new(Vec3::ZERO, 0.5, 2.0)?;
    let query = MovementFallContactQuery {
        trajectory: MovementFallTrajectory::new(MovementFallMode::Normal, -7.95)?,
        launch_height: 0.0,
        elapsed_seconds: 0.0,
        interval_seconds: 0.016,
        horizontal_speed: 7.0,
        support_profile: MovementSupportProfile::PlayerControlled,
    };
    for invalid in [
        MovementFallContactQuery {
            launch_height: f32::NAN,
            ..query
        },
        MovementFallContactQuery {
            elapsed_seconds: -1.0,
            ..query
        },
        MovementFallContactQuery {
            interval_seconds: f32::INFINITY,
            ..query
        },
        MovementFallContactQuery {
            horizontal_speed: -1.0,
            ..query
        },
    ] {
        assert!(matches!(
            invalid.resolve(&volume, Vec3::X, &[]),
            Err(MovementFallContactError::InvalidState)
        ));
    }
    for displacement in [Vec3::ZERO, Vec3::X * 0.0000001, Vec3::X * 0.001] {
        let result = query.resolve(&volume, displacement, &[])?;
        assert_eq!(result.kind, MovementFallContactKind::Clear);
        assert_eq!(result.displacement, displacement);
        assert_eq!(result.consumed_seconds, query.interval_seconds);
        if displacement.x < f32::from_bits(0x3480_0000) {
            assert_eq!(result.distance.to_bits(), displacement.x.to_bits());
        } else {
            assert_eq!(result.distance, 0.0);
        }
    }
    Ok(())
}

/// Reads an exact native float image without decimal conversion.
fn scalar(fields: &mut SplitWhitespace<'_>) -> Result<f32, Box<dyn Error>> {
    Ok(f32::from_bits(u32::from_str_radix(
        fields.next().ok_or("missing float")?,
        16,
    )?))
}

/// Decodes XYZ in the shared collision coordinate space.
fn vector(fields: &mut SplitWhitespace<'_>) -> Result<Vec3, Box<dyn Error>> {
    Ok(Vec3::new(scalar(fields)?, scalar(fields)?, scalar(fields)?))
}
