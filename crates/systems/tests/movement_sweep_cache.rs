//! Original instruction cache hits, endpoint growth, and minimum extrusion.

use glam::Vec3;
use solarity_systems::{MovementCollisionBounds, MovementCollisionVolume, MovementSweepError};
use std::error::Error;

#[test]
fn sweep_cache_matches_original_instructions() -> Result<(), Box<dyn Error>> {
    let mut count = 0;
    let mut misses = 0;
    for (line_number, line) in include_str!("fixtures/movement-sweep-cache-native.txt")
        .lines()
        .enumerate()
    {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let mut fields = line.split_whitespace();
        let miss = fields.next().ok_or("missing decision")? == "1";
        let values = fields
            .map(|field| u32::from_str_radix(field, 16).map(f32::from_bits))
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(values.len(), 21);
        let vector = |index| Vec3::from_slice(&values[index..index + 3]);
        let volume = MovementCollisionVolume::new(vector(0), values[3], values[4])?;
        let cached = MovementCollisionBounds::new(vector(9), vector(12))?;
        let refresh = volume.sweep_refresh_bounds(vector(5), values[8], cached)?;
        assert_eq!(refresh.is_some(), miss, "line {}", line_number + 1);
        let actual = refresh.unwrap_or(cached);
        for (actual, expected) in [
            (actual.minimum(), vector(15)),
            (actual.maximum(), vector(18)),
        ] {
            assert_eq!(
                actual.to_array().map(f32::to_bits),
                expected.to_array().map(f32::to_bits),
                "line {}: {actual:?} != {expected:?}",
                line_number + 1
            );
        }
        misses += usize::from(miss);
        count += 1;
    }
    assert_eq!(count, 1576);
    assert!(misses > 500 && misses < count - 100);
    Ok(())
}

#[test]
fn sweep_cache_rejects_invalid_travel_and_overflow() -> Result<(), Box<dyn Error>> {
    let volume = MovementCollisionVolume::new(Vec3::ZERO, 0.5, 2.)?;
    let cached = MovementCollisionBounds::new(Vec3::splat(-1.), Vec3::splat(3.))?;
    for direction in [Vec3::splat(f32::NAN), Vec3::splat(f32::INFINITY)] {
        assert_eq!(
            volume.sweep_refresh_bounds(direction, 0., cached),
            Err(MovementSweepError::InvalidDisplacement)
        );
    }
    for distance in [f32::NAN, f32::INFINITY] {
        assert_eq!(
            volume.sweep_refresh_bounds(Vec3::X, distance, cached),
            Err(MovementSweepError::InvalidDisplacement)
        );
    }
    assert_eq!(
        volume.sweep_refresh_bounds(Vec3::splat(f32::MAX), 2., cached),
        Err(MovementSweepError::InvalidDisplacement)
    );
    let far = MovementCollisionVolume::new(Vec3::new(f32::MAX, 0., 0.), 0.5, 2.)?;
    assert_eq!(
        far.sweep_refresh_bounds(Vec3::X, f32::MAX, cached),
        Err(MovementSweepError::InvalidCollectionBounds)
    );
    Ok(())
}
