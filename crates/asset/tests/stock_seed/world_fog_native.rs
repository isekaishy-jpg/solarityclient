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

#[test]
fn world_model_scene_fog_matches_original_liquid_flags_and_portal_transition()
-> Result<(), Box<dyn Error>> {
    use crate::{WorldModelFogBank, WorldModelFogPalette};
    let mut count = 0;
    for line in include_str!("../fixtures/world_model_fog_native.txt")
        .lines()
        .filter(|line| line.starts_with("final "))
    {
        let row = line.split_ascii_whitespace().collect::<Vec<_>>();
        let context = WorldFogContext::new(if row[1] == "0" { 0 } else { 530 }, 777.)
            .ok_or("invalid clip")?;
        let palette = WorldModelFogPalette::new(
            u32::from_str_radix(row[2], 16)?,
            [
                WorldModelFogBank::new(300., 0.25, 0xff9a5731),
                WorldModelFogBank::new(40., -0.25, 0xff1a7fdb),
            ],
        );
        let liquid: i32 = row[3].parse()?;
        let distance = if row[4] == "0" {
            None
        } else {
            Some(f32::from_bits(u32::from_str_radix(row[5], 16)?))
        };
        let base = context.finish(500., 0.5, 2.5, Vec3::new(35., 69., 103.) / 255., false);
        let fog = context.world_model_scene(
            base,
            palette,
            distance,
            (liquid >= 0).then_some(liquid as u32),
        );
        let native = row[6..]
            .iter()
            .map(|word| u32::from_str_radix(word, 16))
            .collect::<Result<Vec<_>, _>>()?;
        let color = Vec3::new(
            ((native[0] >> 16) & 255) as f32,
            ((native[0] >> 8) & 255) as f32,
            (native[0] & 255) as f32,
        ) / 255.;
        assert_eq!(fog.color(), color, "{line}");
        for (actual, expected) in [fog.range().0, fog.range().1, fog.exponent()]
            .into_iter()
            .zip(&native[1..])
        {
            check(actual, *expected, line);
        }
        count += 1;
    }
    assert_eq!(count, 1008);
    Ok(())
}
