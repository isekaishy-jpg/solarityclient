//! Original 7EF6E0 state transitions, including native byte rounding.

use super::GlareState;
use crate::{WorldGlareEnvironment, WorldGlareKind};
use glam::Vec3;

/// Decodes the oracle's exact float storage without decimal round trips.
#[allow(clippy::unwrap_used)]
fn float(word: &str) -> f32 {
    let bytes = std::array::from_fn(|i| u8::from_str_radix(&word[i * 2..i * 2 + 2], 16).unwrap());
    f32::from_le_bytes(bytes)
}

/// Compare retained fades and draw properties across ordered stock samples.
#[test]
#[allow(clippy::unwrap_used)]
fn glare_fades_and_angles_match_native_updates() {
    let mut states = [GlareState::default(); 2];
    for row in include_str!("../fixtures/world_glare_native.txt")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let words: Vec<_> = row.split_whitespace().collect();
        let index: usize = words[0].parse().unwrap();
        let values: Vec<_> = words[3..18].iter().map(|s| float(s)).collect();
        let input = WorldGlareEnvironment {
            day: values[0],
            elapsed_seconds: values[1],
            cloud_alpha: [values[2]; 2],
            liquid_depth: (values[3] >= 0.).then_some(values[3]),
            skybox_weight: values[4],
        };
        let sample = states[index].update(
            if index == 0 {
                WorldGlareKind::Sun
            } else {
                WorldGlareKind::Moon
            },
            input,
            Vec3::from_slice(&values[6..9]),
            float(words[19]),
            Vec3::from_slice(&values[9..12]),
            u32::from_str_radix(words[2], 16).unwrap(),
            values[5],
        );
        for (actual, expected) in [
            (sample.size, values[12]),
            (states[index].visibility, values[13]),
            (states[index].lighting_response(), float(words[20])),
        ] {
            assert!(
                actual.to_bits().abs_diff(expected.to_bits()) <= 1,
                "{row}: {actual} != {expected}"
            );
        }
        assert_eq!(
            sample.color,
            u32::from_str_radix(words[18], 16).unwrap(),
            "{row}"
        );
    }
}
