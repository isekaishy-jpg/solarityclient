//! Native support decisions at slope, footprint, and degenerate-edge boundaries.

use std::error::Error;
use std::str::SplitWhitespace;

use glam::Vec3;
use solarity_systems::{MovementCollisionTriangle, MovementSupportProfile, MovementSweepError};

/// Outputs were captured through the original landing predicate and its real
/// unit-policy callee, using either a player object or an ordinary unit object.
#[test]
fn landing_support_matches_original_unit_profiles() -> Result<(), Box<dyn Error>> {
    for line in include_str!("fixtures/movement-support-native.txt").lines() {
        if line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let name = fields.next().ok_or("missing case name")?;
        let profile = match fields.next() {
            Some("0") => MovementSupportProfile::Other,
            Some("1") => MovementSupportProfile::PlayerControlled,
            _ => return Err("invalid support profile".into()),
        };
        let triangle = MovementCollisionTriangle::new([
            vector(&mut fields)?,
            vector(&mut fields)?,
            vector(&mut fields)?,
        ])?;
        let point = vector(&mut fields)?;
        let expected = match fields.next() {
            Some("0") => false,
            Some("1") => true,
            _ => return Err("invalid expected support".into()),
        };
        assert!(fields.next().is_none());
        assert_eq!(
            triangle.supports_at(point, profile)?,
            expected,
            "{name}: {profile:?}"
        );
    }
    Ok(())
}

/// Support admission fails before geometric comparisons consume invalid input.
#[test]
fn landing_support_rejects_nonfinite_foot_points() -> Result<(), Box<dyn Error>> {
    let triangle = MovementCollisionTriangle::new([Vec3::ZERO, Vec3::X, Vec3::Y])?;
    for point in [Vec3::NAN, Vec3::splat(f32::INFINITY)] {
        assert_eq!(
            triangle.supports_at(point, MovementSupportProfile::PlayerControlled),
            Err(MovementSweepError::NonFiniteSupportPoint)
        );
    }
    Ok(())
}

/// Decodes the original float inputs without decimal rounding.
fn vector(fields: &mut SplitWhitespace<'_>) -> Result<Vec3, Box<dyn Error>> {
    let mut values = [0.0; 3];
    for value in &mut values {
        *value = f32::from_bits(u32::from_str_radix(
            fields.next().ok_or("missing coordinate")?,
            16,
        )?);
    }
    Ok(Vec3::from_array(values))
}
