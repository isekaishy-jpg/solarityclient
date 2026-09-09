//! Original 7BDB10/78F570/791CB0 results, captured without running the client.

use glam::{Mat4, Vec3};

use super::distance::SceneryDistance;

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
