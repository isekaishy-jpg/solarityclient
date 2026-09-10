//! Camera immersion reaches authored global/local banks and liquid overrides.

use std::error::Error;

use glam::Vec3;
use solarity_asset::{
    ArchiveCatalog, AssetStore, ClientDataRoot, LightCatalog, LiquidTypeCatalog, Locale,
};
use solarity_ecs::{ActiveWorld, WorldBootstrap, WorldMapId};
use solarity_network::WorldTimeSpeed;
use solarity_runtime::{RealmClock, RuntimeWorldEnvironment, RuntimeWorldEnvironmentError};
use solarity_systems::SubmergedLiquid;

use crate::support::ClientFixture;

/// 7F3230 shares the immersion result with presentation, selects bank one for
/// both global/local volumes, and bypasses them for a direct parameter override.
#[test]
fn environment_uses_camera_liquid_bank_depth_and_parameter_override() -> Result<(), Box<dyn Error>>
{
    let mut rows = vec![13, 571, 0, 0, 0, 0, 0, 1, 2, 0, 0, 0, 0, 0, 0];
    rows.extend([
        14,
        571,
        ((17_066.666_f32 - 20.) * 36.).to_bits(),
        (30_f32 * 36.).to_bits(),
        ((17_066.666_f32 - 10.) * 36.).to_bits(),
        (100_f32 * 36.).to_bits(),
        (200_f32 * 36.).to_bits(),
        3,
        4,
        0,
        0,
        0,
        0,
        0,
        0,
    ]);
    // The fallback deliberately differs from map 571's global and locals.
    rows.extend([1, 0, 0, 0, 0, 0, 0, 5, 2, 0, 0, 0, 0, 0, 0]);
    for row in rows.as_chunks_mut::<15>().0 {
        row[9] = if row[0] == 14 { 1 } else { 3 };
        row[10] = if row[0] == 14 { 2 } else { 4 };
        row[11] = if row[0] == 14 { 5 } else { 3 };
    }
    let mut parameters = Vec::new();
    let mut colors = Vec::new();
    let mut floats = Vec::new();
    for (index, color) in [0x101010, 0x202020, 0x404040, 0x808080, 0xf0f0f0]
        .into_iter()
        .enumerate()
    {
        let id = index as u32 + 1;
        parameters.extend([id, 0, 0, 0, 0, 0, 0, 0, 0]);
        for channel in 0..18 {
            colors.extend(band((id - 1) * 18 + channel + 1, color));
        }
        for channel in 0..6 {
            let value = match channel {
                0 => 18_000_f32,
                1 => 0.5,
                _ => 0.,
            };
            floats.extend(band((id - 1) * 6 + channel + 1, value.to_bits()));
        }
    }
    let mut liquids = Vec::new();
    for (id, override_id, maximum) in [(1, 0, 10_f32), (2, 5, 0.), (3, 999, 0.)] {
        let mut row = [0; 45];
        row[0] = id;
        row[6] = maximum.to_bits();
        row[7..10].fill(0.5_f32.to_bits());
        row[10] = override_id;
        liquids.extend(row);
    }
    let (fog_root, fog_group) = indoor_fog_model();
    let fixture = ClientFixture::with_common_files(&[
        ("World\\Fog.wmo", &fog_root),
        ("World\\Fog_000.wmo", &fog_group),
        (
            "DBFilesClient\\Weather.dbc",
            &table(8, &[7, 9, 1, 1_f32.to_bits(), 0, 0, 0, 0]),
        ),
        ("DBFilesClient\\Light.dbc", &table(15, &rows)),
        ("DBFilesClient\\LightParams.dbc", &table(9, &parameters)),
        ("DBFilesClient\\LightIntBand.dbc", &table(34, &colors)),
        ("DBFilesClient\\LightFloatBand.dbc", &table(34, &floats)),
        (
            "DBFilesClient\\ScreenEffect.dbc",
            &table(
                10,
                &[
                    141,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    4,
                    0,
                    0,
                    81,
                    0,
                    2,
                    0,
                    0,
                    0,
                    0,
                    u32::MAX,
                    0,
                    0,
                    142,
                    0,
                    99,
                    0,
                    0,
                    0,
                    0,
                    u32::MAX,
                    0,
                    0,
                ],
            ),
        ),
        ("DBFilesClient\\LiquidType.dbc", &table(45, &liquids)),
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let liquids = LiquidTypeCatalog::load(&mut store)?;
    let fog_model = solarity_asset::DecodedWorldModel::load(
        &mut store,
        &solarity_asset::AssetPath::new("World\\Fog.wmo")?,
    )?;
    let fog_placement = solarity_systems::PlacedWorldModelCollision::prepare_transform(
        std::sync::Arc::new(fog_model),
        glam::Mat4::IDENTITY,
    )?;
    let indoor_fog = fog_placement.fog_environment(0, None, Vec3::ZERO)?;
    let mut environment = RuntimeWorldEnvironment::new(LightCatalog::load(&mut store)?, 8 << 30)?
        .with_screen_effects(solarity_asset::ScreenEffectCatalog::load(&mut store)?)
        .with_weather(solarity_asset::WeatherCatalog::load(&mut store)?);
    let clock = RealmClock::new(WorldTimeSpeed::new(0, 0., 0)?);
    for (position, exterior, underwater) in [
        (Vec3::new(10., 20., 30.), 64., 128.),
        (Vec3::new(1000., 20., 30.), 16., 32.),
    ] {
        let world = ActiveWorld::enter(WorldBootstrap::new(
            WorldMapId::new(571),
            1,
            "LiquidLight",
            position,
            0.,
        ));
        let frame = environment
            .synchronize(Some(&world), Some(&clock))?
            .ok_or("environment")?;
        assert_eq!(frame.light().ambient_color(), Vec3::splat(exterior / 255.));
        assert_eq!(frame.fog().range(), (388.5, 777.));
        assert_eq!(frame.fog().exponent(), 4.25);
        let indoor = frame.with_world_model_fog(indoor_fog);
        assert_eq!(indoor.fog().range(), (25., 777.));
        assert_eq!(indoor.fog().exponent(), 6.175);
        assert_eq!(
            indoor.fog().color(),
            Vec3::new(0x12 as f32, 0x34 as f32, 0x56 as f32) / 255.
        );
        assert_eq!(indoor.light(), frame.light());
        assert_eq!(frame.ordinary_model_fog(), frame.fog());
        assert_eq!(indoor.ordinary_model_fog().color(), frame.fog().color());
        assert_eq!(indoor.ordinary_model_fog().range(), indoor.fog().range());
        assert_eq!(
            indoor.ordinary_model_fog().exponent(),
            indoor.fog().exponent()
        );
        assert_eq!(frame.with_world_model_fog(None), frame);
        assert!(!frame.has_camera_liquid());
        assert_eq!(environment.resolve_liquid(frame, None, &liquids)?, frame);
        for (depth, expected) in [
            (-0.001, underwater),
            (0., underwater),
            (10., underwater / 2.),
            (50., underwater / 2.),
        ] {
            let resolved = environment.resolve_liquid(
                frame,
                Some(SubmergedLiquid {
                    liquid_type: 1,
                    surface_height: position.z + depth,
                    depth,
                }),
                &liquids,
            )?;
            assert!(resolved.has_camera_liquid());
            assert_eq!(
                resolved.light().ambient_color(),
                Vec3::splat(expected / 255.)
            );
            assert_eq!(
                resolved.light().diffuse_color(),
                resolved.light().ambient_color()
            );
            assert_eq!(
                resolved.light().fog_color(),
                resolved.light().ambient_color()
            );
            assert_eq!(resolved.fog().range(), (388.5, 777.));
            assert_eq!(resolved.fog().exponent(), 8.5);
            assert_eq!(resolved.fog().color(), Vec3::splat(underwater / 255.));
            let indoor = resolved.with_world_model_fog(indoor_fog);
            assert_eq!(indoor.fog().range(), (25., 777.));
            assert_eq!(indoor.fog().exponent(), 13.45);
            assert_eq!(
                indoor.fog().color(),
                Vec3::new(0xab as f32, 0xcd as f32, 0xef as f32) / 255.
            );
            assert_eq!(indoor.light(), resolved.light());
            assert_eq!(resolved.ordinary_model_fog(), resolved.fog());
            assert_eq!(indoor.ordinary_model_fog().color(), resolved.fog().color());
            assert_eq!(indoor.ordinary_model_fog().range(), indoor.fog().range());
            assert_eq!(
                indoor.ordinary_model_fog().exponent(),
                indoor.fog().exponent()
            );
            assert_eq!(resolved.position(), position);
            assert_eq!(resolved.view_distance(), frame.view_distance());
            assert_eq!(environment.current(), Some(frame));
        }
        let resolved = environment.resolve_liquid(
            frame,
            Some(SubmergedLiquid {
                liquid_type: 2,
                surface_height: 50.,
                depth: 20.,
            }),
            &liquids,
        )?;
        assert_eq!(resolved.light().ambient_color(), Vec3::splat(240. / 255.));
        assert_eq!(resolved.fog().exponent(), 8.5);
        assert!(
            environment
                .resolve_liquid(
                    frame,
                    Some(SubmergedLiquid {
                        liquid_type: 3,
                        surface_height: 50.,
                        depth: 20.
                    }),
                    &liquids
                )
                .is_err()
        );
        assert_eq!(
            environment.resolve_liquid(
                frame,
                Some(SubmergedLiquid {
                    liquid_type: 99,
                    surface_height: 50.,
                    depth: 20.
                }),
                &liquids
            ),
            Err(RuntimeWorldEnvironmentError::MissingLiquidType { id: 99 })
        );
    }
    // Entering RFC used to terminate the event loop on MissingGlobalLight.
    // Check repeated transfers, underwater selection, and exterior restoration.
    for (map, expected) in [(389, 240.), (571, 16.), (389, 240.)] {
        let position = Vec3::new(1000., 20., 30.);
        let world = ActiveWorld::enter(WorldBootstrap::new(
            WorldMapId::new(map),
            1,
            "MapTransfer",
            position,
            0.,
        ));
        let frame = environment
            .synchronize(Some(&world), Some(&clock))?
            .ok_or("map transfer environment")?;
        assert_eq!(frame.light().ambient_color(), Vec3::splat(expected / 255.));
        let underwater = environment.resolve_liquid(
            frame,
            Some(SubmergedLiquid {
                liquid_type: 1,
                surface_height: position.z,
                depth: 0.,
            }),
            &liquids,
        )?;
        assert_eq!(underwater.light().ambient_color(), Vec3::splat(32. / 255.));
        assert_eq!(
            frame.fog().range(),
            if map < 530 {
                (250., 500.)
            } else {
                (388.5, 777.)
            }
        );
        assert_eq!(frame.fog().exponent(), if map < 530 { 1. } else { 4.25 });
        assert_eq!(
            underwater.fog().exponent(),
            if map < 530 { 1. } else { 8.5 }
        );
        assert_eq!(environment.resolve_liquid(frame, None, &liquids)?, frame);
    }
    let world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        1,
        "Weather",
        Vec3::new(1000., 20., 30.),
        0.,
    ));
    environment.receive_weather(
        solarity_network::WorldWeatherUpdate {
            weather_id: 7,
            grade: 1.,
            instant: true,
        },
        0,
    );
    let frame = environment
        .synchronize(Some(&world), Some(&clock))?
        .ok_or("weather frame")?;
    assert_eq!(frame.weather_blend(), 1.);
    assert_eq!(frame.light().ambient_color(), Vec3::splat(64. / 255.));
    for (liquid_type, expected) in [(1, 128.), (2, 240.)] {
        let resolved = environment.resolve_liquid(
            frame,
            Some(SubmergedLiquid {
                liquid_type,
                surface_height: 30.,
                depth: 0.,
            }),
            &liquids,
        )?;
        assert_eq!(
            resolved.light().ambient_color(),
            Vec3::splat(expected / 255.)
        );
    }
    environment.receive_weather(
        solarity_network::WorldWeatherUpdate {
            weather_id: 999,
            grade: 0.,
            instant: true,
        },
        0,
    );
    let clear = environment
        .synchronize(Some(&world), Some(&clock))?
        .ok_or("clear frame")?;
    assert_eq!(clear.weather_blend(), 0.);
    assert_eq!(clear.light().ambient_color(), Vec3::splat(16. / 255.));
    environment.receive_weather(
        solarity_network::WorldWeatherUpdate {
            weather_id: 7,
            grade: 1.,
            instant: true,
        },
        0,
    );
    environment.disconnect();
    let cleared = environment
        .synchronize(Some(&world), Some(&clock))?
        .ok_or("reset frame")?;
    assert_eq!(cleared.weather_blend(), 0.);
    assert_eq!(cleared.light(), clear.light());
    environment.select_screen_effect(141);
    let overridden = environment
        .synchronize(Some(&world), Some(&clock))?
        .ok_or("screen effect frame")?;
    assert_eq!(overridden.light().ambient_color(), Vec3::splat(64. / 255.));
    assert_eq!(overridden.light().global_skybox(), Some(0));
    for (liquid_type, expected, global) in [(1, 64., Some(0)), (2, 240., None)] {
        let resolved = environment.resolve_liquid(
            overridden,
            Some(SubmergedLiquid {
                liquid_type,
                surface_height: 30.,
                depth: 0.,
            }),
            &liquids,
        )?;
        assert_eq!(
            resolved.light().ambient_color(),
            Vec3::splat(expected / 255.)
        );
        assert_eq!(resolved.light().global_skybox(), global);
    }
    environment.set_full_screen_effects(false);
    environment.select_screen_effect(81);
    let manual = environment
        .synchronize(Some(&world), Some(&clock))?
        .ok_or("manual fog")?;
    assert!(!manual.sky_enabled());
    assert_eq!(manual.fog().range(), (105., 150.));
    assert_eq!(manual.fog().exponent(), 6.505);
    assert_eq!(manual.fog().color(), Vec3::new(76., 76., 99.) / 255.);
    assert_eq!(manual.light(), clear.light()); // Manual fog leaves horizon/palette words intact.
    environment.set_full_screen_effects(true);
    let retained = environment
        .synchronize(Some(&world), Some(&clock))?
        .ok_or("retained manual")?;
    assert_eq!(retained.fog(), manual.fog());
    environment.select_screen_effect(142);
    let unknown = environment
        .synchronize(Some(&world), Some(&clock))?
        .ok_or("unknown type")?;
    assert_eq!(unknown.fog(), manual.fog());
    assert!(!unknown.sky_enabled());
    for liquid_type in [1, 2] {
        let wet = environment.resolve_liquid(
            manual,
            Some(SubmergedLiquid {
                liquid_type,
                surface_height: 30.,
                depth: 100.,
            }),
            &liquids,
        )?;
        assert_eq!(wet.fog().range(), manual.fog().range());
        assert_eq!(wet.fog().color(), manual.fog().color());
        assert_eq!(wet.fog().exponent(), manual.fog().exponent() * 2.);
        assert!(!wet.sky_enabled());
        let indoors = wet.with_world_model_fog(indoor_fog);
        assert_eq!(indoors.ordinary_model_fog().color(), manual.fog().color());
        assert_eq!(indoors.fog().range(), (25., 777.));
        assert!(!indoors.sky_enabled());
    }
    environment.select_screen_effect(81);
    let refreshed = environment
        .synchronize(Some(&world), Some(&clock))?
        .ok_or("refreshed manual")?;
    assert_eq!(refreshed.fog().color(), Vec3::ONE);
    environment.set_view_distance(300.);
    let clipped = environment
        .synchronize(Some(&world), Some(&clock))?
        .ok_or("changed farclip")?;
    assert_eq!(clipped.view_distance().value(), 300.);
    assert_eq!(clipped.fog().exponent(), refreshed.fog().exponent());
    environment.select_screen_effect(81);
    let relatched = environment
        .synchronize(Some(&world), Some(&clock))?
        .ok_or("relatched curve")?;
    assert_eq!(relatched.fog().exponent(), 4.525);
    environment.set_view_distance(777.);
    environment.select_screen_effect(141);
    let restored = environment
        .synchronize(Some(&world), Some(&clock))?
        .ok_or("restored fog")?;
    assert!(restored.sky_enabled());
    assert_eq!(restored.fog(), overridden.fog());
    for id in [0, 999] {
        environment.select_screen_effect(id);
        assert_eq!(
            environment
                .synchronize(Some(&world), Some(&clock))?
                .ok_or("clear effect frame")?
                .light(),
            clear.light()
        );
    }
    environment.select_screen_effect(141);
    environment.disconnect();
    assert_eq!(
        environment
            .synchronize(Some(&world), Some(&clock))?
            .ok_or("disconnect effect frame")?
            .light(),
        clear.light()
    );
    Ok(())
}

/// Authored dry/wet banks in a closed interior, independent of Light.dbc.
fn indoor_fog_model() -> (Vec<u8>, Vec<u8>) {
    fn chunk(bytes: &mut Vec<u8>, name: [u8; 4], payload: &[u8]) {
        bytes.extend(name);
        bytes.extend((payload.len() as u32).to_le_bytes());
        bytes.extend(payload);
    }
    let mut root = Vec::new();
    chunk(&mut root, *b"REVM", &17u32.to_le_bytes());
    let mut header = [0; 64];
    header[4..8].copy_from_slice(&1u32.to_le_bytes());
    chunk(&mut root, *b"DHOM", &header);
    let mut info = [0; 32];
    info[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
    chunk(&mut root, *b"IGOM", &info);
    let mut fog = [0; 48];
    for (offset, value) in [(24, 100f32), (28, 0.25), (36, 50.), (40, 0.5)] {
        fog[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    fog[32..36].copy_from_slice(&0xff123456u32.to_le_bytes());
    fog[44..48].copy_from_slice(&0xffabcdefu32.to_le_bytes());
    chunk(&mut root, *b"GOFM", &fog);
    let mut group = Vec::new();
    chunk(&mut group, *b"REVM", &17u32.to_le_bytes());
    chunk(&mut group, *b"PGOM", &[0; 68]);
    (root, group)
}

/// One exact constant cyclic band.
fn band(id: u32, value: u32) -> [u32; 34] {
    let mut row = [0; 34];
    row[0] = id;
    row[1] = 1;
    row[18] = value;
    row
}

/// Encodes a complete stock table with only an empty string.
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
