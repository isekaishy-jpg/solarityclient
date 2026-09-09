//! Independent native body-box clipping and averaged presentation normals.

use glam::Vec3;
use solarity_systems::{MovementCollisionTriangle, MovementCollisionVolume};
use std::{error::Error, str::SplitWhitespace};

#[test]
fn body_box_ground_normals_match_unhooked_original() -> Result<(), Box<dyn Error>> {
    for (index, line) in include_str!("fixtures/movement-ground-normal-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .enumerate()
    {
        let mut fields = line.split_whitespace();
        let position = vector(&mut fields)?;
        let radius = scalar(&mut fields)?;
        let height = scalar(&mut fields)?;
        let count: usize = fields.next().ok_or("missing triangle count")?.parse()?;
        let mut triangles = Vec::with_capacity(count);
        for _ in 0..count {
            let normal = vector(&mut fields)?;
            let vertices = [
                vector(&mut fields)?,
                vector(&mut fields)?,
                vector(&mut fields)?,
            ];
            triangles.push(MovementCollisionTriangle::with_normal(vertices, normal)?);
        }
        let expected = vector(&mut fields)?;
        let actual =
            MovementCollisionVolume::new(position, radius, height)?.ground_normal(&triangles);
        assert_eq!(
            actual.to_array().map(f32::to_bits),
            expected.to_array().map(f32::to_bits),
            "native case {index}"
        );
        assert!(fields.next().is_none());
    }
    Ok(())
}

/// Decodes exact stored float bits without decimal conversion.
fn scalar(fields: &mut SplitWhitespace<'_>) -> Result<f32, Box<dyn Error>> {
    Ok(f32::from_bits(u32::from_str_radix(
        fields.next().ok_or("missing float")?,
        16,
    )?))
}

/// Reads one ordered XYZ fixture field.
fn vector(fields: &mut SplitWhitespace<'_>) -> Result<Vec3, Box<dyn Error>> {
    Ok(Vec3::new(scalar(fields)?, scalar(fields)?, scalar(fields)?))
}
