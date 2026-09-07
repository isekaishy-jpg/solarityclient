//! Original 7F3230 depth/color output through real WDBC catalog sampling.

use std::error::Error;

use glam::Vec3;
use solarity_asset::{
    ArchiveCatalog, AssetStore, ClientDataRoot, LightCatalog, LiquidTypeCatalog, Locale,
    WorldLightCondition, WorldLightQuery,
};

use crate::support::{Fixture, FixtureFile};

/// Packed fog/ambient/direct channels match original x87 execution exactly.
#[test]
fn liquid_depth_matches_original_environment_colors() -> Result<(), Box<dyn Error>> {
    let (cases, remainder) = include_bytes!("../fixtures/liquid_light_depth.bin").as_chunks::<36>();
    assert!(remainder.is_empty());
    assert_eq!(cases.len(), 3584);
    let mut parameters = Vec::new();
    let mut colors = Vec::new();
    let mut floats = Vec::new();
    let mut liquids = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let (words, _) = case.as_chunks::<4>();
        let words = words
            .iter()
            .copied()
            .map(u32::from_le_bytes)
            .collect::<Vec<_>>();
        let id = index as u32 + 1;
        parameters.extend([id, 0, 0, 0, 0, 0, 0, 0, 0]);
        for channel in 0..18 {
            colors.extend(band((id - 1) * 18 + channel + 1, words[0]));
        }
        for channel in 0..6 {
            floats.extend(band((id - 1) * 6 + channel + 1, 0));
        }
        let mut liquid = [0; 45];
        liquid[0] = id;
        liquid[6..10].copy_from_slice(&words[2..6]);
        liquid[10] = id;
        liquids.extend(liquid);
    }
    let tables = [
        ("DBFilesClient\\Light.dbc", table(15, &[])),
        ("DBFilesClient\\LightParams.dbc", table(9, &parameters)),
        ("DBFilesClient\\LightSkybox.dbc", table(3, &[])),
        ("DBFilesClient\\LightIntBand.dbc", table(34, &colors)),
        ("DBFilesClient\\LightFloatBand.dbc", table(34, &floats)),
        ("DBFilesClient\\LiquidType.dbc", table(45, &liquids)),
    ];
    let fixture = Fixture::new(
        &tables
            .iter()
            .map(|(path, bytes)| FixtureFile {
                archive: "common.MPQ",
                path,
                bytes,
            })
            .collect::<Vec<_>>(),
    )?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let lights = LightCatalog::load(&mut store)?;
    let liquids = LiquidTypeCatalog::load(&mut store)?;
    for (index, case) in cases.iter().enumerate() {
        let (words, _) = case.as_chunks::<4>();
        let words = words
            .iter()
            .copied()
            .map(u32::from_le_bytes)
            .collect::<Vec<_>>();
        let liquid = liquids.entry(index as u32 + 1).ok_or("liquid")?;
        let base = lights.sample_parameter(liquid.light_id(), 0)?;
        let sample = base.with_liquid_depth(liquid, f32::from_bits(words[1]));
        for (actual, expected) in [
            sample.fog_color(),
            sample.ambient_color(),
            sample.diffuse_color(),
        ]
        .into_iter()
        .zip(&words[6..])
        {
            assert_eq!(
                actual,
                unpack(*expected),
                "native liquid depth case {index}, input {words:x?}"
            );
        }
        assert_eq!(sample.fog_range(), base.fog_range());
        assert_eq!(sample.sky_colors(), base.sky_colors());
        assert_eq!(sample.liquid_colors(), base.liquid_colors());
    }
    Ok(())
}

/// Native 7EB070 returns opaque black and 7EAEF0 returns zero for empty bands.
/// Underwater parameter 213 includes the zero-key specular band 3826 seen in
/// the swimming crash. Nonzero padded values must not become sampled keys.
#[test]
fn underwater_empty_light_bands_preserve_stock_defaults() -> Result<(), Box<dyn Error>> {
    let mut colors = Vec::new();
    let mut floats = Vec::new();
    for id in [213_u32, 214] {
        for channel in 0..18 {
            let mut row = band((id - 1) * 18 + channel + 1, 0x0011_2233);
            row[1] = u32::from(id == 214);
            colors.extend(row);
        }
        for channel in 0..6 {
            let mut row = band((id - 1) * 6 + channel + 1, 17.25_f32.to_bits());
            row[1] = u32::from(id == 214);
            floats.extend(row);
        }
    }
    let tables = [
        (
            "DBFilesClient\\Light.dbc",
            table(15, &[1, 1, 0, 0, 0, 0, 0, 214, 213, 0, 0, 0, 0, 0, 0]),
        ),
        (
            "DBFilesClient\\LightParams.dbc",
            table(
                9,
                &[213, 0, 0, 0, 0, 0, 0, 0, 0, 214, 0, 0, 0, 0, 0, 0, 0, 0],
            ),
        ),
        ("DBFilesClient\\LightSkybox.dbc", table(3, &[])),
        ("DBFilesClient\\LightIntBand.dbc", table(34, &colors)),
        ("DBFilesClient\\LightFloatBand.dbc", table(34, &floats)),
    ];
    let fixture = Fixture::new(
        &tables
            .iter()
            .map(|(path, bytes)| FixtureFile {
                archive: "common.MPQ",
                path,
                bytes,
            })
            .collect::<Vec<_>>(),
    )?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let lights = LightCatalog::load(&mut store)?;
    for time in [0, 719, 1440, 2879] {
        let query = WorldLightQuery::new(1, Vec3::ZERO, time);
        let exterior = lights.sample(query)?;
        assert_eq!(exterior.diffuse_color(), unpack(0x0011_2233));
        let underwater = lights.sample(query.with_condition(WorldLightCondition::UNDERWATER))?;
        assert_eq!(underwater, lights.sample_parameter(213, time)?);
        assert_eq!(underwater.ambient_color(), Vec3::ZERO);
        assert_eq!(underwater.diffuse_color(), Vec3::ZERO);
        assert_eq!(underwater.fog_color(), Vec3::ZERO);
        assert_eq!(underwater.specular_color(), Vec3::ZERO);
        assert_eq!(underwater.sky_colors(), [Vec3::ZERO; 5]);
        assert_eq!(underwater.liquid_colors(), [Vec3::ZERO; 4]);
        assert_eq!(underwater.sky_floats(), [0.; 4]);
        assert_eq!(underwater.fog_range(), (0., 10.));
        let model = lights.model_light_colors(213, time)?;
        assert_eq!(model.ambient(), Vec3::ZERO);
        assert_eq!(model.diffuse(), Vec3::ZERO);
        assert_eq!(lights.sample(query)?, exterior);
    }
    Ok(())
}

/// One constant cyclic band, retaining the complete 16-key storage.
fn band(id: u32, value: u32) -> [u32; 34] {
    let mut row = [0; 34];
    row[0] = id;
    row[1] = 1;
    row[18] = value;
    row
}

/// Exact stock WDBC layout with an empty string block.
fn table(width: usize, words: &[u32]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for word in [
        words.len() as u32 / width as u32,
        width as u32,
        width as u32 * 4,
        1,
    ]
    .iter()
    .chain(words)
    {
        bytes.extend(word.to_le_bytes());
    }
    bytes.push(0);
    bytes
}

/// Public environment colors use normalized RGB after native 8-bit packing.
fn unpack(color: u32) -> Vec3 {
    Vec3::new(
        ((color >> 16) & 255) as f32,
        ((color >> 8) & 255) as f32,
        (color & 255) as f32,
    ) / 255.0
}
