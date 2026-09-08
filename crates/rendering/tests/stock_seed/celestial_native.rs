//! Independent native ephemeris captures including table boundaries and calendar phase.

use super::*;
use std::error::Error;

fn floats(raw: &str) -> Result<Vec<f32>, Box<dyn Error>> {
    let bytes = raw
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| Ok(u8::from_str_radix(std::str::from_utf8(p)?, 16)?))
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    Ok(bytes
        .as_chunks::<4>()
        .0
        .iter()
        .copied()
        .map(f32::from_le_bytes)
        .collect())
}

#[test]
fn celestials_match_original_positions_sizes_and_daylight() -> Result<(), Box<dyn Error>> {
    let mut count = 0;
    for line in include_str!("../fixtures/world_celestial_native.txt")
        .lines()
        .filter(|l| l.starts_with("sample "))
    {
        let p = line.split_whitespace().collect::<Vec<_>>();
        let input = floats(p[1])?;
        let expected = floats(p[2])?;
        let actual =
            WorldCelestials::sample(input[0], input[1], Vec3::new(input[2], input[3], input[4]));
        for (i, body) in actual.bodies().into_iter().enumerate() {
            for (a, b) in body
                .position()
                .to_array()
                .into_iter()
                .zip(&expected[i * 4..i * 4 + 3])
            {
                assert!(
                    (a - b).abs() <= 0.000_002_f32.max(b.abs() * 0.000_000_12),
                    "position case {count}, body {i}, input {input:?}: {a} != {b}"
                );
            }
            assert!(
                (body.size() - expected[i * 4 + 3]).abs() <= 0.000_001,
                "size case {count}, body {i}, input {input:?}: {} != {}",
                body.size(),
                expected[i * 4 + 3]
            );
        }
        assert!(
            (actual.daylight() - expected[12]).abs() <= 0.000_001,
            "daylight case {count}, input {input:?}: {} != {}",
            actual.daylight(),
            expected[12]
        );
        count += 1;
    }
    assert!(count >= 1000);
    Ok(())
}
