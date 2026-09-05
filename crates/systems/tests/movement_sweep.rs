//! Golden contacts captured by executing the original build-12340 narrow phase.

use std::error::Error;
use std::str::SplitWhitespace;

use glam::Vec3;
use solarity_systems::{MovementCollisionTriangle, MovementCollisionVolume, MovementSweepError};

/// These outputs come from original x86 execution, independent of the Rust
/// implementation. Small arithmetic differences reflect x87 versus f32 storage.
#[test]
fn body_sweeps_match_original_x86_contacts() -> Result<(), Box<dyn Error>> {
    for line in include_str!("fixtures/movement-sweep-native.txt").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let mut fields = line.split_whitespace();
        let name = fields.next().ok_or("missing case name")?;
        let origin = vector(&mut fields)?;
        let radius = scalar(&mut fields)?;
        let height = scalar(&mut fields)?;
        let displacement = vector(&mut fields)?;
        let triangle_count: usize = fields.next().ok_or("missing triangle count")?.parse()?;
        let triangles = (0..triangle_count)
            .map(|_| {
                Ok(MovementCollisionTriangle::new([
                    vector(&mut fields)?,
                    vector(&mut fields)?,
                    vector(&mut fields)?,
                ])?)
            })
            .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
        let expected_distance = scalar(&mut fields)?;
        let expected_triangle: i32 = fields.next().ok_or("missing selected triangle")?.parse()?;
        let expected_planes: usize = fields.next().ok_or("missing plane count")?.parse()?;
        let result = MovementCollisionVolume::new(origin, radius, height)?
            .sweep(displacement, &triangles)?;
        assert!(
            (result.distance() - expected_distance).abs() < 0.0001,
            "{name}: expected {expected_distance}, got {}",
            result.distance()
        );
        assert_eq!(
            result.last_triangle(),
            usize::try_from(expected_triangle).ok(),
            "{name}"
        );
        assert_eq!(
            result.planes().len(),
            expected_planes,
            "{name}: {:?}",
            result.planes()
        );
        for plane in result.planes() {
            let normal = vector(&mut fields)?;
            let offset = scalar(&mut fields)?;
            assert!(
                (plane.normal() - normal).length() < 0.0001,
                "{name}: expected {normal}, got {:?}",
                plane.normal()
            );
            assert!((plane.offset() - offset).abs() < 0.0001, "{name}");
        }
        assert!(fields.next().is_none(), "{name}: trailing fixture fields");
    }
    Ok(())
}

/// Invalid inputs cannot introduce NaNs into plane comparisons or clipping.
#[test]
fn sweep_geometry_rejects_nonfinite_and_degenerate_inputs() -> Result<(), Box<dyn Error>> {
    for (origin, radius, height) in [
        (Vec3::NAN, 0.5, 2.0),
        (Vec3::ZERO, f32::NAN, 2.0),
        (Vec3::ZERO, -0.5, 2.0),
        (Vec3::ZERO, 0.0, 2.0),
        (Vec3::ZERO, 0.5, 0.1),
        (Vec3::ZERO, 0.5, f32::INFINITY),
    ] {
        assert!(matches!(
            MovementCollisionVolume::new(origin, radius, height),
            Err(MovementSweepError::InvalidVolume)
        ));
    }
    for vertices in [[Vec3::ZERO; 3], [Vec3::ZERO, Vec3::X, Vec3::NAN]] {
        assert!(matches!(
            MovementCollisionTriangle::new(vertices),
            Err(MovementSweepError::InvalidTriangle)
        ));
    }
    let volume = MovementCollisionVolume::new(Vec3::ZERO, 0.5, 2.0)?;
    for displacement in [Vec3::NAN, Vec3::splat(f32::INFINITY), Vec3::splat(f32::MAX)] {
        assert!(matches!(
            volume.sweep(displacement, &[]),
            Err(MovementSweepError::InvalidDisplacement)
        ));
    }
    Ok(())
}

/// Decodes an exact native f32 image without decimal-rounding fixture drift.
fn scalar(fields: &mut SplitWhitespace<'_>) -> Result<f32, Box<dyn Error>> {
    Ok(f32::from_bits(u32::from_str_radix(
        fields.next().ok_or("missing scalar")?,
        16,
    )?))
}

/// Reads one vector in the query's world-coordinate order.
fn vector(fields: &mut SplitWhitespace<'_>) -> Result<Vec3, Box<dyn Error>> {
    Ok(Vec3::new(scalar(fields)?, scalar(fields)?, scalar(fields)?))
}
