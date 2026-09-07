//! Original map-handle containment across residency states and float boundaries.

use std::error::Error;
use std::str::SplitWhitespace;

use glam::Vec3;
use solarity_systems::{MovementCollisionBounds, MovementTransportVolume};

#[test]
fn passenger_retention_matches_original_m2_and_wmo_handles() -> Result<(), Box<dyn Error>> {
    let mut count = 0;
    for line in include_str!("fixtures/passenger-containment-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let mut fields = line.split_whitespace();
        let flags = integer(&mut fields)?;
        let present = integer(&mut fields)?;
        let ready = integer(&mut fields)?;
        let loaded_groups = integer(&mut fields)?;
        let plane_count = integer(&mut fields)?;
        let bounds = MovementCollisionBounds::new(vector(&mut fields)?, vector(&mut fields)?)?;
        let position = vector(&mut fields)?;
        let planes = (0..plane_count)
            .map(|_| {
                Ok([
                    scalar(&mut fields)?,
                    scalar(&mut fields)?,
                    scalar(&mut fields)?,
                    scalar(&mut fields)?,
                ])
            })
            .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
        let expected = integer(&mut fields)? != 0;
        let volume = if flags & 8 != 0 {
            MovementTransportVolume::WorldModel {
                loaded_groups,
                planes: &planes,
            }
        } else if flags & 0x40 != 0 {
            MovementTransportVolume::Model((present != 0 && ready != 0).then_some(bounds))
        } else {
            MovementTransportVolume::Unbounded
        };
        assert_eq!(
            volume.contains(position),
            expected,
            "native case {count}: {line}"
        );
        assert!(fields.next().is_none());
        count += 1;
    }
    assert_eq!(count, 880);
    Ok(())
}

/// Decimal handle metadata preserves native zero/nonzero gates.
fn integer(fields: &mut SplitWhitespace<'_>) -> Result<usize, Box<dyn Error>> {
    Ok(fields.next().ok_or("missing integer")?.parse()?)
}

/// The capture stores float bits to keep adjacent upper-plane boundaries distinct.
fn scalar(fields: &mut SplitWhitespace<'_>) -> Result<f32, Box<dyn Error>> {
    Ok(f32::from_bits(u32::from_str_radix(
        fields.next().ok_or("missing float")?,
        16,
    )?))
}

/// Read one native C3Vector in XYZ order.
fn vector(fields: &mut SplitWhitespace<'_>) -> Result<Vec3, Box<dyn Error>> {
    Ok(Vec3::new(scalar(fields)?, scalar(fields)?, scalar(fields)?))
}
