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
            frame.depth_bin(bounds)?.map_or(-1, i32::from),
            expected,
            "native case {index}"
        );
        assert!(fields.next().is_none());
    }
    Ok(())
}

/// 792AD0 shares the depth formula; transformed roots use a separate overlap list.
#[test]
fn outdoor_world_model_depth_lists_match_unhooked_original() -> Result<(), Box<dyn Error>> {
    let mut compared = 0;
    for (index, line) in include_str!("fixtures/world-model-scene-depth-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .enumerate()
    {
        let mut fields = line.split_whitespace();
        let root_flags: u32 = fields.next().ok_or("missing root flags")?.parse()?;
        let group_flags: u32 = fields.next().ok_or("missing group flags")?.parse()?;
        // These captures also retain list-kind evidence. Only static eligible
        // groups consume the common depth formula tested by this public API.
        if root_flags & 0x400 != 0 || group_flags & 0x10008 == 0 {
            continue;
        }
        let frame = WorldSceneDepthFrame::new(vector(&mut fields)?, vector(&mut fields)?)?;
        let bounds = [vector(&mut fields)?, vector(&mut fields)?];
        let expected: i32 = fields.next().ok_or("missing bucket")?.parse()?;
        assert_eq!(
            frame.depth_bin(bounds)?.map_or(-1, i32::from),
            expected,
            "native WMO case {index}"
        );
        assert!(fields.next().is_none());
        compared += 1;
    }
    assert_eq!(compared, 603);
    Ok(())
}

/// 792BD0 retains an out-of-range transformed entry in the original overlap list.
#[test]
fn transformed_world_model_depth_conversion_matches_unhooked_original() -> Result<(), Box<dyn Error>>
{
    let mut count = 0;
    for line in include_str!("fixtures/world-model-scene-overlap-depth-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let mut fields = line.split_whitespace();
        let frame = WorldSceneDepthFrame::new(vector(&mut fields)?, vector(&mut fields)?)?;
        let bounds = [vector(&mut fields)?, vector(&mut fields)?];
        let expected: i32 = fields.next().ok_or("missing destination")?.parse()?;
        assert_eq!(
            frame.depth_bin(bounds)?.map_or(-2, i32::from),
            expected,
            "native moving WMO case {count}"
        );
        assert!(fields.next().is_none());
        count += 1;
    }
    assert_eq!(count, 489);
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
