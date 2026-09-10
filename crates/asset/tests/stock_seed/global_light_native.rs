//! Captured 7F3230 copies verify every retained and replaced palette channel.

use super::*;
use std::error::Error;

fn sample(prefix: u32) -> WorldLightSample {
    let word = |index: u32| prefix + index;
    let float = |index| f32::from_bits(word(index));
    let color = |index| {
        let v = word(index);
        Vec3::new(
            ((v >> 16) & 255) as f32,
            ((v >> 8) & 255) as f32,
            (v & 255) as f32,
        ) / 255.
    };
    WorldLightSample {
        fog_near: float(18) * float(19),
        fog_far: float(18),
        fog_ratio: float(19),
        fog_exponent: float(20),
        fog_color: color(8),
        ambient_color: color(0),
        diffuse_color: color(1),
        specular_color: color(9),
        sky_colors: [3, 4, 5, 6, 7].map(color),
        additional_colors: [2, 10, 11, 12, 13].map(color),
        highlight_sky: float(21),
        glow: float(22),
        sky_floats: [23, 24, 25, 26].map(float),
        liquid_colors: [14, 15, 16, 17].map(color),
        liquid_alphas: [27, 28, 29, 30].map(float),
        skyboxes: [31, 33, 35].map(|i| SkyboxBlend {
            id: word(i),
            weight: float(i + 1),
        }),
        global_skybox: None,
        cloud_type_id: word(37),
        cloud_type_weight: float(38),
    }
}

#[test]
fn global_light_replacement_matches_native_palette_retention() -> Result<(), Box<dyn Error>> {
    let base = sample(0x3f000000);
    let replacement = sample(0x40000000);
    let mut count = 0;
    for line in include_str!("../fixtures/global_light_native.txt").lines() {
        let Some(line) = line.strip_prefix("palette ") else {
            continue;
        };
        let row = line.split_whitespace().collect::<Vec<_>>();
        let mut current = base;
        if row[3] == "1" {
            current.replace_global_palette(replacement);
        }
        let native = row[4..]
            .iter()
            .map(|word| u32::from_str_radix(word, 16))
            .collect::<Result<Vec<_>, _>>()?;
        let f = |i| f32::from_bits(native[i]);
        for (value, i) in [
            current.fog_far,
            current.fog_ratio,
            current.fog_exponent,
            current.highlight_sky,
            current.glow,
        ]
        .into_iter()
        .zip(18..23)
        .chain(current.sky_floats.into_iter().zip(23..27))
        .chain(current.liquid_alphas.into_iter().zip(27..31))
        {
            assert_eq!(value.to_bits(), native[i], "{line}: field {i}");
        }
        for (channel, i) in [1, 0, 3, 4, 5, 6, 7, 8, 2, 9, 10, 11, 12, 13, 14, 15, 16, 17]
            .into_iter()
            .enumerate()
        {
            let v = native[i];
            assert_eq!(
                current.color_channel(channel),
                Some(
                    Vec3::new(
                        ((v >> 16) & 255) as f32,
                        ((v >> 8) & 255) as f32,
                        (v & 255) as f32
                    ) / 255.
                )
            );
        }
        for (slot, i) in current.skyboxes.into_iter().zip([31, 33, 35]) {
            assert_eq!((slot.id, slot.weight), (native[i], f(i + 1)));
        }
        assert_eq!(
            (current.cloud_type_id, current.cloud_type_weight),
            (native[37], f(38))
        );
        count += 1;
    }
    assert_eq!(count, 9);
    Ok(())
}
