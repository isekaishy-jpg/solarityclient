//! Collision travel agrees with the original executable's float stores.

use glam::Vec3;
use solarity_systems::{MovementIntervalDrive, MovementTravelAxes};
use std::{error::Error, str::SplitWhitespace};

#[test]
fn interval_drive_matches_original_unhooked_instruction_ranges() -> Result<(), Box<dyn Error>> {
    for (index, line) in include_str!("fixtures/movement-interval-drive-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .enumerate()
    {
        let mut fields = line.split_whitespace();
        let delta = Vec3::new(
            float(&mut fields)?,
            float(&mut fields)?,
            float(&mut fields)?,
        );
        let duration = integer(&mut fields)?;
        let remaining = integer(&mut fields)?;
        let axes = match integer(&mut fields)? {
            0 => MovementTravelAxes::Horizontal,
            1 => MovementTravelAxes::Spatial,
            _ => return Err("invalid travel axes".into()),
        };
        let speed = float(&mut fields)?;
        let direction = Vec3::new(
            float(&mut fields)?,
            float(&mut fields)?,
            float(&mut fields)?,
        );
        let distance = float(&mut fields)?;
        let drive = MovementIntervalDrive::new(delta, duration, axes)?;
        assert_eq!(drive.speed().to_bits(), speed.to_bits(), "speed {index}");
        assert_eq!(
            drive.direction().to_array().map(f32::to_bits),
            direction.to_array().map(f32::to_bits),
            "direction {index}"
        );
        assert_eq!(
            drive.distance(remaining).to_bits(),
            distance.to_bits(),
            "distance {index}"
        );
        assert!(fields.next().is_none());
    }
    Ok(())
}

/// Reads an unsigned native clock or branch selection.
fn integer(fields: &mut SplitWhitespace<'_>) -> Result<u32, Box<dyn Error>> {
    Ok(fields.next().ok_or("missing integer")?.parse()?)
}

/// Decodes one exact native float store.
fn float(fields: &mut SplitWhitespace<'_>) -> Result<f32, Box<dyn Error>> {
    Ok(f32::from_bits(u32::from_str_radix(
        fields.next().ok_or("missing float")?,
        16,
    )?))
}
