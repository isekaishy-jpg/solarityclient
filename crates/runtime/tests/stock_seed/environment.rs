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
            floats.extend(band((id - 1) * 6 + channel + 1, 0));
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
    let fixture = ClientFixture::with_common_files(&[
        ("DBFilesClient\\Light.dbc", &table(15, &rows)),
        ("DBFilesClient\\LightParams.dbc", &table(9, &parameters)),
        ("DBFilesClient\\LightIntBand.dbc", &table(34, &colors)),
        ("DBFilesClient\\LightFloatBand.dbc", &table(34, &floats)),
        ("DBFilesClient\\LiquidType.dbc", &table(45, &liquids)),
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let liquids = LiquidTypeCatalog::load(&mut store)?;
    let mut environment = RuntimeWorldEnvironment::new(LightCatalog::load(&mut store)?, 8 << 30)?;
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
    Ok(())
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
