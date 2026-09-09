//! Native scene insertion is distinct from the subsequent draw visibility test.

use glam::Vec3;
use solarity_systems::WorldSceneDepthFrame;
use std::{error::Error, str::SplitWhitespace};

#[test]
fn outdoor_m2_depth_lists_match_unhooked_original() -> Result<(), Box<dyn Error>> {
    for (index, line) in include_str!("fixtures/scene-depth-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .enumerate()
    {
        let mut fields = line.split_whitespace();
        let frame = WorldSceneDepthFrame::new(vector(&mut fields)?, vector(&mut fields)?)?;
        let bounds = [vector(&mut fields)?, vector(&mut fields)?];
        let expected: i32 = fields.next().ok_or("missing bucket")?.parse()?;
        assert_eq!(
            frame.m2_depth_bin(bounds)?.map_or(-1, i32::from),
            expected,
            "native case {index}"
        );
        assert!(fields.next().is_none());
    }
    Ok(())
}

/// Decodes a native XYZ float store without decimal conversion.
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
