//! Unmodified 12340 fog routines, including exact threshold and clip boundaries.

use super::WorldFogContext;
use glam::Vec3;
use std::error::Error;

fn check(actual: f32, expected: u32, line: &str) {
    if f32::from_bits(expected).is_nan() {
        assert!(actual.is_nan(), "{line}: expected NaN, got {actual}");
    } else {
        assert_eq!(actual.to_bits(), expected, "{line}: got {actual}");
    }
}

#[test]
fn fog_policy_matches_original_parameter_final_and_wmo_routines() -> Result<(), Box<dyn Error>> {
    let mut counts = [0; 3];
    for line in include_str!("../fixtures/world_fog_policy_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let mut row = line.split_ascii_whitespace();
        let kind = row.next().ok_or("missing native row kind")?;
        let words = row
            .map(|word| u32::from_str_radix(word, 16))
            .collect::<Result<Vec<_>, _>>()?;
        let context = WorldFogContext::new(
            if words[1] == 0 { 0 } else { 530 },
            f32::from_bits(words[0]),
        )
        .ok_or("invalid native clip")?;
        let f = |i: usize| f32::from_bits(words[i]);
        match kind {
            "palette" => {
                counts[0] += 1;
                let (end, ratio, exponent) = context.palette(f(2), f(3));
                for (actual, expected) in [end, ratio, exponent].into_iter().zip(&words[4..]) {
                    check(actual, *expected, line);
                }
            }
            "final" => {
                counts[1] += 1;
                let color = Vec3::new(18., 52., 86.) / 255.;
                let fog = context.finish(f(3), f(4), f(5), color, words[2] != 0);
                assert_eq!(words[6], 0xff123456);
                assert_eq!(fog.color(), color);
                for (actual, expected) in [fog.range().0, fog.range().1, fog.exponent()]
                    .into_iter()
                    .zip(&words[7..])
                {
                    check(actual, *expected, line);
                }
            }
            "wmo" => {
                counts[2] += 1;
                let color = Vec3::new(171., 205., 239.) / 255.;
                let fog = context.world_model(f(2), f(3), color);
                assert_eq!(words[4], 0xffabcdef);
                assert_eq!(fog.color(), color);
                for (actual, expected) in [fog.range().0, fog.range().1, fog.exponent()]
                    .into_iter()
                    .zip(&words[5..])
                {
                    check(actual, *expected, line);
                }
            }
            _ => panic!("unknown native fixture row {line}"),
        }
    }
    assert_eq!(counts, [1078, 2156, 1078]);
    Ok(())
}

#[test]
fn context_rejects_invalid_clips_and_selects_the_native_map_boundary() -> Result<(), Box<dyn Error>>
{
    for clip in [0., -1., f32::NAN, f32::INFINITY] {
        assert!(WorldFogContext::new(530, clip).is_none());
    }
    assert!(
        !WorldFogContext::new(529, 777.)
            .ok_or("valid clip rejected")?
            .uses_power_curve()
    );
    assert!(
        WorldFogContext::new(530, 777.)
            .ok_or("valid clip rejected")?
            .uses_power_curve()
    );
    Ok(())
}
