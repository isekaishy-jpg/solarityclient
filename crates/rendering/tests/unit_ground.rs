//! Original x86 smoothing, model tilt, animation blend, and scaled placement.

use glam::Vec3;
use solarity_rendering::M2GroundNormal;
use std::{error::Error, str::SplitWhitespace};

#[test]
fn ground_smoothing_matches_original_unit_callback() -> Result<(), Box<dyn Error>> {
    for (index, line) in
        cases(include_str!("fixtures/unit-ground-smoothing-native.txt")).enumerate()
    {
        let mut fields = line.split_whitespace();
        let old = vector(&mut fields)?;
        let target = vector(&mut fields)?;
        let delta = scalar(&mut fields)?;
        let expected = vector(&mut fields)?;
        let mut state = M2GroundNormal::default();
        state.advance(old, 1000.0)?;
        state.advance(target, delta)?;
        assert_eq!(
            state.normal().to_array().map(f32::to_bits),
            expected.to_array().map(f32::to_bits),
            "smoothing case {index}"
        );
    }
    Ok(())
}

#[test]
fn ground_model_matrices_match_original_tilt_and_sequence_blend() -> Result<(), Box<dyn Error>> {
    for (index, line) in cases(include_str!("fixtures/unit-ground-pose-native.txt")).enumerate() {
        let mut fields = line.split_whitespace();
        let normal = vector(&mut fields)?;
        let position = vector(&mut fields)?;
        let yaw = scalar(&mut fields)?;
        let scale = scalar(&mut fields)?;
        let flags: u32 = fields.next().ok_or("missing model flags")?.parse()?;
        for _ in 0..6 {
            fields.next().ok_or("missing timer field")?;
        }
        let weight = scalar(&mut fields)?;
        let mut state = M2GroundNormal::default();
        state.advance(normal, 1000.0)?;
        let actual = state
            .transform(position, yaw, scale, flags, weight)?
            .to_cols_array();
        for (component, actual) in actual.into_iter().enumerate() {
            let expected = scalar(&mut fields)?;
            assert_eq!(
                actual.to_bits(),
                expected.to_bits(),
                "placement case {index}, component {component}: {actual} versus {expected}"
            );
        }
        assert!(fields.next().is_none());
    }
    Ok(())
}

/// Skips fixture metadata while retaining deterministic case indices.
fn cases(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
}

/// Reads stored float words without decimal precision loss.
fn scalar(fields: &mut SplitWhitespace<'_>) -> Result<f32, Box<dyn Error>> {
    Ok(f32::from_bits(u32::from_str_radix(
        fields.next().ok_or("missing float")?,
        16,
    )?))
}

/// Decodes a fixture's ordered XYZ triple.
fn vector(fields: &mut SplitWhitespace<'_>) -> Result<Vec3, Box<dyn Error>> {
    Ok(Vec3::new(scalar(fields)?, scalar(fields)?, scalar(fields)?))
}
