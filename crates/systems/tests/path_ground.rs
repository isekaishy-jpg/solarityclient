//! Spline collision preserves small corrections and native travel normals.

use glam::Vec3;
use solarity_systems::MovementSplineTarget;
use std::{error::Error, str::SplitWhitespace};

#[test]
fn path_ground_transitions_match_original_instruction_ranges() -> Result<(), Box<dyn Error>> {
    for (index, line) in include_str!("fixtures/path-ground-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .enumerate()
    {
        let mut fields = line.split_whitespace();
        let start = vector(&mut fields)?;
        let target = vector(&mut fields)?;
        let duration = integer(&mut fields)?;
        let flags = integer(&mut fields)?;
        let forced = integer(&mut fields)?;
        let pre_snap = integer(&mut fields)? != 0;
        let post_snap = integer(&mut fields)? != 0;
        let normal = vector(&mut fields)?;
        let sample = MovementSplineTarget::new(start, target, duration)?;
        assert_eq!(
            sample.requires_snap(flags),
            pre_snap,
            "pre-collision {index}"
        );
        if forced == 0 {
            assert_eq!(
                sample.corrected_position(start)?,
                if post_snap { target } else { start },
                "post-collision {index}"
            );
        }
        assert_eq!(
            sample
                .unavailable_geometry_normal()
                .to_array()
                .map(f32::to_bits),
            normal.to_array().map(f32::to_bits),
            "travel normal {index}"
        );
        assert!(fields.next().is_none());
    }
    Ok(())
}

/// Reads a native unsigned scalar.
fn integer(fields: &mut SplitWhitespace<'_>) -> Result<u32, Box<dyn Error>> {
    Ok(fields.next().ok_or("missing integer")?.parse()?)
}

/// Decodes exact float storage from the native fixture.
fn vector(fields: &mut SplitWhitespace<'_>) -> Result<Vec3, Box<dyn Error>> {
    let mut result = [0.; 3];
    for value in &mut result {
        *value = f32::from_bits(u32::from_str_radix(
            fields.next().ok_or("missing float")?,
            16,
        )?);
    }
    Ok(Vec3::from_array(result))
}
