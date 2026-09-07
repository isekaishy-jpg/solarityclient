//! Complete native Path.cpp geometry, including the out-of-line point cache.

use glam::Vec3;
use solarity_systems::{MovementPath, MovementPathMode};

#[test]
fn path_geometry_matches_original_native_length_position_and_direction()
-> Result<(), Box<dyn std::error::Error>> {
    let mut count = 0;
    for line in include_str!("fixtures/movement-path-native.txt").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let fields: Vec<_> = line.split(" | ").collect();
        let mut input = fields[0].split_whitespace();
        assert_eq!(input.next(), Some("path"));
        let mode = match input.next() {
            Some("0") => MovementPathMode::Linear,
            Some("1") => MovementPathMode::Smooth,
            _ => return Err("invalid native mode".into()),
        };
        let nodes: Vec<_> = input
            .map(float)
            .collect::<Result<Vec<_>, _>>()?
            .as_chunks::<3>()
            .0
            .iter()
            .map(|point| Vec3::from_array(*point))
            .collect();
        let path = MovementPath::new(nodes, mode)?;
        assert_close(path.length(), float(fields[1])?, "length", line);
        let expected: Vec<_> = fields[2]
            .split_whitespace()
            .map(float)
            .collect::<Result<_, _>>()?;
        let actual = path.sample(expected[0], Vec3::new(0.3, 0.4, 0.5));
        for (actual, expected) in actual
            .position
            .to_array()
            .into_iter()
            .chain(actual.direction.to_array())
            .zip(&expected[1..])
        {
            assert_close(actual, *expected, "sample", line);
        }
        count += 1;
    }
    assert_eq!(count, 200);
    Ok(())
}

fn float(word: &str) -> Result<f32, std::num::ParseIntError> {
    u32::from_str_radix(word, 16).map(f32::from_bits)
}

fn assert_close(actual: f32, expected: f32, field: &str, line: &str) {
    let tolerance = 0.00001_f32.max(expected.abs() * 0.000001);
    assert!(
        (actual - expected).abs() <= tolerance,
        "{field}: actual {actual}, native {expected}: {line}"
    );
}
