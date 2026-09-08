//! Original day-band samples and ordered local-light overlays through WDBC.

use crate::support::{Fixture, FixtureFile};
use glam::Vec3;
use solarity_asset::{
    ArchiveCatalog, AssetStore, ClientDataRoot, LightCatalog, Locale, WorldLightQuery,
    WorldLightSample,
};
use std::error::Error;

fn unhex(value: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}

fn table(width: u32, records: &[u8]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for word in [records.len() as u32 / (width * 4), width, width * 4, 1] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes.extend_from_slice(records);
    bytes.push(0);
    bytes
}

fn compare(sample: WorldLightSample, bytes: &[u8], line: &str) {
    let words = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .copied()
        .map(u32::from_le_bytes)
        .collect::<Vec<_>>();
    for (channel, native) in [1, 0, 3, 4, 5, 6, 7, 8, 2, 9, 10, 11, 12, 13, 14, 15, 16, 17]
        .into_iter()
        .enumerate()
    {
        let expected = Vec3::new(
            ((words[native] >> 16) & 255) as f32,
            ((words[native] >> 8) & 255) as f32,
            (words[native] & 255) as f32,
        ) / 255.0;
        assert_eq!(
            sample.color_channel(channel),
            Some(expected),
            "{line}: channel {channel}"
        );
    }
    assert_eq!(sample.color_channel(18), None);
    assert_eq!(sample.cloud_type(), (words[37], f32::from_bits(words[38])));
    let colors = [
        sample.ambient_color(),
        sample.diffuse_color(),
        sample.fog_color(),
        sample.specular_color(),
    ];
    for (color, index) in colors.into_iter().zip([0, 1, 8, 9]) {
        let components = color
            .to_array()
            .map(|v| (v * 255.0).round_ties_even() as u32);
        assert_eq!(
            components[0] << 16 | components[1] << 8 | components[2],
            words[index] & 0xffffff,
            "{line}: color {index}"
        );
    }
    for (color, index) in sample
        .sky_colors()
        .into_iter()
        .zip(3..8)
        .chain(sample.liquid_colors().into_iter().zip(14..18))
    {
        let expected = Vec3::new(
            ((words[index] >> 16) & 255) as f32,
            ((words[index] >> 8) & 255) as f32,
            (words[index] & 255) as f32,
        ) / 255.0;
        assert_eq!(color, expected, "{line}: color {index}");
    }
    let f = |index| f32::from_bits(words[index]);
    let (near, far) = sample.fog_range();
    assert!(
        (far - f(18)).abs() < 0.0001,
        "{line}: fog far {far} != {}",
        f(18)
    );
    assert!((near - f(18) * f(19)).abs() < 0.0001, "{line}: fog near");
    for (actual, index) in [sample.highlight_sky(), sample.glow()]
        .into_iter()
        .zip([21, 22])
        .chain(sample.sky_floats().into_iter().zip([23, 24, 25, 26]))
        .chain(sample.liquid_alphas().into_iter().zip(27..31))
    {
        assert!(
            (actual - f(index)).abs() < 0.00001,
            "{line}: scalar {index}: {actual} != {}",
            f(index)
        );
    }
    for (slot, index) in sample.skyboxes().into_iter().zip([31, 33, 35]) {
        assert_eq!(slot.id(), words[index], "{line}: skybox ID");
        assert_eq!(slot.weight(), f(index + 1), "{line}: skybox weight");
    }
}

#[test]
fn world_light_matches_original_cyclic_sampling_and_ordered_overlays() -> Result<(), Box<dyn Error>>
{
    let fixture_text = include_str!("../fixtures/world_light_sampling_native.txt");
    let mut parameters = Vec::new();
    let mut colors = Vec::new();
    let mut floats = Vec::new();
    for line in fixture_text
        .lines()
        .filter(|line| line.starts_with("parameter "))
    {
        let row = line.split_ascii_whitespace().collect::<Vec<_>>();
        parameters.extend(unhex(row[2])?);
        colors.extend(unhex(row[3])?);
        floats.extend(unhex(row[4])?);
    }
    let mut light_words = Vec::new();
    for map in 1..=3_u32 {
        light_words.extend([map * 100, map, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]);
        for index in 0..map {
            light_words.extend([
                map * 100 + index + 1,
                map,
                614400_f32.to_bits(),
                0,
                614400_f32.to_bits(),
                0,
                (256_f32 * 36.).to_bits(),
                index + 2,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
            ]);
        }
    }
    for map in [4, 5] {
        light_words.extend([map * 100, map, 0, 0, 0, 0, 0, 1, 1, 2, 2, 0, 0, 0, 0]);
    }
    light_words.extend([
        501,
        5,
        614400_f32.to_bits(),
        0,
        614400_f32.to_bits(),
        0,
        (256_f32 * 36.).to_bits(),
        3,
        3,
        4,
        4,
        0,
        0,
        0,
        0,
    ]);
    let light_bytes = light_words
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    let tables = [
        ("DBFilesClient/Light.dbc", table(15, &light_bytes)),
        ("DBFilesClient/LightParams.dbc", table(9, &parameters)),
        ("DBFilesClient/LightSkybox.dbc", table(3, &[])),
        ("DBFilesClient/LightIntBand.dbc", table(34, &colors)),
        ("DBFilesClient/LightFloatBand.dbc", table(34, &floats)),
    ];
    let files = tables
        .iter()
        .map(|(path, bytes)| FixtureFile {
            archive: "common.MPQ",
            path,
            bytes,
        })
        .collect::<Vec<_>>();
    let fixture = Fixture::new(&files)?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let lights = LightCatalog::load(&mut store)?;
    let mut counts = [0; 3];
    for line in fixture_text
        .lines()
        .chain(include_str!("../fixtures/world_weather_palette_native.txt").lines())
    {
        let row = line.split_ascii_whitespace().collect::<Vec<_>>();
        match row.first().copied() {
            Some("sample") => {
                compare(
                    lights.sample_parameter(row[1].parse()?, row[2].parse()?)?,
                    &unhex(row[3])?,
                    line,
                );
                counts[0] += 1;
            }
            Some("overlay") => {
                let time = row[1].parse()?;
                let numerator: f32 = row[2].parse()?;
                let map = row[3].split(',').count() as u32;
                let sample = lights.sample(WorldLightQuery::new(
                    map,
                    Vec3::new(256.0 - numerator, 0., 0.),
                    time,
                ))?;
                compare(sample, &unhex(row[4])?, line);
                counts[1] += 1;
            }
            Some("weather") => {
                let time = row[1].parse()?;
                let weight = f32::from_bits(u32::from_str_radix(row[2], 16)?);
                let numerator: f32 = row[3].parse()?;
                for condition in [
                    solarity_asset::WorldLightCondition::EXTERIOR,
                    solarity_asset::WorldLightCondition::UNDERWATER,
                ] {
                    let sample = lights.sample(
                        WorldLightQuery::new(
                            if numerator == 0. { 4 } else { 5 },
                            Vec3::new(256. - numerator, 0., 0.),
                            time,
                        )
                        .with_condition(condition)
                        .with_weather(weight),
                    )?;
                    compare(sample, &unhex(row[4])?, line);
                }
                counts[2] += 1;
            }
            _ => {}
        }
    }
    assert_eq!(counts, [68, 231, 392]);
    Ok(())
}
