//! Original 7BDB10/78F570/791CB0 results, captured without running the client.

use glam::{Mat4, Vec3};

use super::distance::SceneryDistance;

/// Collision-only scenery retains a point instead of expanding its empty box.
#[test]
fn scenery_bounds_and_size_class_match_original_inverted_box_rules()
-> Result<(), Box<dyn std::error::Error>> {
    let mut count = 0;
    for line in include_str!("fixtures/scenery_bounds_native.txt")
        .lines()
        .skip(1)
    {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        let bits = fields[..28]
            .iter()
            .map(|field| u32::from_str_radix(field, 16))
            .collect::<Result<Vec<_>, _>>()?;
        let values = bits.iter().copied().map(f32::from_bits).collect::<Vec<_>>();
        let minimum = Vec3::from_slice(&values[..3]);
        let maximum = Vec3::from_slice(&values[3..6]);
        let transform = Mat4::from_cols_array(values[6..22].try_into()?);
        let (low, high) = SceneryDistance::world_bounds(minimum, maximum, transform);
        let actual = low
            .to_array()
            .into_iter()
            .chain(high.to_array())
            .map(f32::to_bits)
            .collect::<Vec<_>>();
        assert_eq!(actual, bits[22..28], "{line}");
        let category = fields[28].parse::<usize>()?;
        let scenery = SceneryDistance::new(minimum, maximum, transform);
        for (minimum_class, depth) in [0., 30., 100., 200., 750.].into_iter().enumerate() {
            assert_eq!(
                scenery.admits_group(depth, 1.),
                category >= minimum_class,
                "{line}"
            );
        }
        count += 1;
    }
    assert_eq!(count, 294);
    Ok(())
}

/// Shadow distance cuts off before the ordinary fade band, including equality.
#[test]
fn scenery_shadow_distance_matches_native_fade_start() -> Result<(), Box<dyn std::error::Error>> {
    let mut count = 0;
    for line in include_str!("fixtures/scenery_shadow_distance_native.txt")
        .lines()
        .filter_map(|line| line.strip_prefix("shadow_distance "))
    {
        let values = line
            .split_whitespace()
            .map(str::parse::<f32>)
            .collect::<Result<Vec<_>, _>>()?;
        let extent = [1., 4., 15., 100., 101.][values[0] as usize];
        let center = Vec3::from_slice(&values[2..5]);
        let scenery = SceneryDistance::new(
            Vec3::splat(-extent * 0.5),
            Vec3::splat(extent * 0.5),
            Mat4::from_translation(center),
        );
        assert_eq!(
            scenery.admits_shadow(Vec3::from_slice(&values[5..8]), values[1]),
            values[8] != 0.,
            "{line}"
        );
        count += 1;
    }
    assert_eq!(count, 1050);
    Ok(())
}

/// Every size class consumes the original minimum-class result through admission.
#[test]
fn minimum_doodad_class_matches_original_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    let mut count = 0;
    for line in include_str!("../../../systems/tests/fixtures/world_model_doodad_depth_native.txt")
        .lines()
        .filter_map(|line| line.strip_prefix("class "))
    {
        let fields: Vec<_> = line.split_whitespace().collect();
        let detail = f32::from_bits(u32::from_str_radix(fields[0], 16)?);
        let depth = f32::from_bits(u32::from_str_radix(fields[1], 16)?);
        let minimum = fields[2].parse::<usize>()?;
        for (category, extent) in [1., 4., 15., 100., 101.].into_iter().enumerate() {
            let scenery = SceneryDistance::new(Vec3::ZERO, Vec3::splat(extent), Mat4::IDENTITY);
            assert_eq!(
                scenery.admits_group(depth, detail),
                category >= minimum,
                "{line}, class {category}"
            );
        }
        count += 1;
    }
    assert_eq!(count, 80);
    Ok(())
}

#[test]
fn scenery_size_transform_detail_and_alpha_match_original_admission()
-> Result<(), Box<dyn std::error::Error>> {
    let mut count = 0;
    for row in include_str!("fixtures/scenery_distance_native.txt")
        .lines()
        .filter(|row| !row.starts_with('#'))
    {
        let values = row
            .split_whitespace()
            .map(str::parse::<f32>)
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(values.len(), 27);
        let matrix: [f32; 16] = values[6..22].try_into()?;
        let scenery = SceneryDistance::new(
            Vec3::from_slice(&values[..3]),
            Vec3::from_slice(&values[3..6]),
            Mat4::from_cols_array(&matrix),
        );
        let actual = scenery.opacity(Vec3::from_slice(&values[22..25]), values[25]);
        let expected = values[26];
        assert!(
            (actual - expected).abs() < 0.00002,
            "row {count}: opacity {actual}, original {expected}: {row}"
        );
        if expected == 0.0 || expected == 1.0 {
            assert_eq!(actual, expected, "row {count}: visibility snap");
        }
        count += 1;
    }
    assert_eq!(count, 486);
    Ok(())
}
