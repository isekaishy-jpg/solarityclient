//! Direct executable evidence for model/liquid pass classification.

use glam::{Mat4, Vec3, Vec4};
use solarity_rendering::{M2LiquidState, M2TransparentPass};

#[test]
fn model_liquid_view_plane_matches_complete_native_query_preparation()
-> Result<(), std::num::ParseIntError> {
    let mut count = 0;
    for line in include_str!("../fixtures/model_liquid_plane_native.txt").lines() {
        if line.starts_with('#') {
            continue;
        }
        let values: Vec<_> = line
            .split_whitespace()
            .map(|word| u32::from_str_radix(word, 16).map(f32::from_bits))
            .collect::<Result<_, _>>()?;
        let M2LiquidState::Surface(actual) =
            M2LiquidState::at_world_height(values[0], Mat4::from_cols_slice(&values[1..17]))
        else {
            panic!("surface plane");
        };
        let expected = Vec4::from_slice(&values[17..21]);
        assert!(
            actual.abs_diff_eq(expected, 0.000_1),
            "case {count}: {actual:?} != {expected:?}"
        );
        count += 1;
    }
    assert_eq!(count, 16);
    Ok(())
}

#[test]
fn model_liquid_classification_matches_original_sphere_boundaries()
-> Result<(), std::num::ParseIntError> {
    let mut count = 0;
    for line in include_str!("../fixtures/model_liquid_native.txt").lines() {
        if line.starts_with('#') {
            continue;
        }
        let fields: Vec<_> = line.split_whitespace().collect();
        let values: Vec<_> = fields[3..30]
            .iter()
            .map(|word| u32::from_str_radix(word, 16).map(f32::from_bits))
            .collect::<Result<_, _>>()?;
        let center = (Vec3::from_slice(&values[..3]) + Vec3::from_slice(&values[3..6])) * 0.5;
        let plane = Vec4::from_slice(&values[23..27]);
        let state = match fields[0] {
            "32" => M2LiquidState::Above,
            "64" => M2LiquidState::Below,
            "96" => M2LiquidState::Surface(plane),
            _ => panic!("unexpected native liquid flags"),
        };
        let passes = state.classify_model(
            center,
            values[6],
            Mat4::from_cols_slice(&values[7..23]),
            fields[1] == "1",
            fields[2] == "1",
        );
        assert_eq!(
            (passes.above(), passes.below()),
            (fields[30] == "1", fields[31] == "1"),
            "case {count}: {line}"
        );
        if passes.above() && passes.below() {
            assert_eq!(passes.clip_plane(M2TransparentPass::One), Some(plane));
            assert_eq!(passes.clip_plane(M2TransparentPass::Two), Some(-plane));
        } else {
            assert_eq!(passes.clip_plane(M2TransparentPass::One), None);
            assert_eq!(passes.clip_plane(M2TransparentPass::Two), None);
        }
        count += 1;
    }
    assert_eq!(count, 972);
    Ok(())
}
