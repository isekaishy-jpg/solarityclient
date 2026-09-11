//! Submerged liquid admission over decoded archive data.

use super::support::{Fixture, FixtureFile};
use super::world_model_registration::{chunk_data_mut, floor_fixture, portal_fixture};
use glam::{Mat4, Vec3};
use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale, MapCatalog, TerrainMap};
use solarity_asset::{AssetPath, DecodedWorldModel, LiquidTypeCatalog};
use solarity_systems::PlacedWorldModelCollision;
use solarity_systems::{WorldModelCameraRegistrationQuery, WorldModelRegistrationKind};

/// The camera's root query uses all ray faces and clears exterior selections.
#[test]
fn camera_water_registration_matches_original_root_and_portal_queries() -> Result<(), Box<dyn Error>>
{
    for (case, line) in include_str!("../fixtures/liquid-camera-oracle.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .enumerate()
    {
        let words = line.split_whitespace().collect::<Vec<_>>();
        let settings = words[..8]
            .iter()
            .map(|word| word.parse::<i32>())
            .collect::<Result<Vec<_>, _>>()?;
        let values = words[8..16]
            .iter()
            .map(|word| u32::from_str_radix(word, 16).map(f32::from_bits))
            .collect::<Result<Vec<_>, _>>()?;
        let (mut root, _, mut adjacent) =
            portal_fixture([0., 0., 1.], values[7], settings[7] as i16, false, 2);
        let mut group = floor_fixture(settings[0] as usize, 0, 0);
        if settings[6] != 0 {
            adjacent = group.clone();
        }
        let info = chunk_data_mut(&mut root, *b"IGOM");
        for index in 0..2 {
            info[index * 32..index * 32 + 4]
                .copy_from_slice(&(settings[1 + index * 2] as u32).to_le_bytes());
            for (axis, value) in [-3f32, -3., -1., 3., 3., 3.].into_iter().enumerate() {
                info[index * 32 + 4 + axis * 4..index * 32 + 8 + axis * 4]
                    .copy_from_slice(&value.to_le_bytes());
            }
        }
        let header = chunk_data_mut(&mut group, *b"PGOM");
        header[8..12].copy_from_slice(&(settings[2] as u32).to_le_bytes());
        header[38..40].copy_from_slice(&1u16.to_le_bytes());
        chunk_data_mut(&mut adjacent, *b"PGOM")[8..12]
            .copy_from_slice(&(settings[4] as u32).to_le_bytes());
        let fixture = Fixture::new(&[
            FixtureFile {
                path: "World\\Camera.wmo",
                bytes: &root,
            },
            FixtureFile {
                path: "World\\Camera_000.wmo",
                bytes: &group,
            },
            FixtureFile {
                path: "World\\Camera_001.wmo",
                bytes: &adjacent,
            },
        ])?;
        let mut store = AssetStore::mount(ArchiveCatalog::discover(
            ClientDataRoot::new(fixture.data_root())?,
            Locale::EnUs,
        )?)?;
        let model = Arc::new(DecodedWorldModel::load(
            &mut store,
            &AssetPath::new("World\\Camera.wmo")?,
        )?);
        let mut placement = PlacedWorldModelCollision::prepare_transform(model, Mat4::IDENTITY)?;
        let mut camera = WorldModelCameraRegistrationQuery::new(
            Vec3::from_slice(&values[..3]),
            Vec3::from_slice(&values[3..6]),
            values[6],
        )?;
        camera.probe_root(
            1,
            if settings[5] == 0 {
                WorldModelRegistrationKind::Static
            } else {
                WorldModelRegistrationKind::Transformed
            },
            &mut placement,
        )?;
        let camera = camera.finish_scene();
        assert_eq!(
            camera.is_some(),
            words[16] == "1",
            "camera admission case {case}: {line}"
        );
        if let Some(camera) = camera {
            // 7D59B0 promotes a transformed-only hit into the primary slot.
            assert!(camera.secondary.is_none(), "single-root case {case}");
            let camera = camera.primary;
            assert_eq!(
                camera.group,
                words[17].parse::<usize>()?,
                "camera group case {case}: {line}"
            );
            let secondary = words[18].parse::<u32>()?;
            assert_eq!(
                camera.secondary_group,
                (secondary != u32::from(u16::MAX)).then_some(secondary as usize),
                "camera secondary group case {case}: {line}"
            );

            // 7D59B0 retains independent banks regardless of root visit order.
            // The two decoded roots overlap exactly, exercising the equal-hit
            // case that must not let one bank replace the other.
            for order in [[false, true], [true, false]] {
                let mut query = WorldModelCameraRegistrationQuery::new(
                    Vec3::from_slice(&values[..3]),
                    Vec3::from_slice(&values[3..6]),
                    values[6],
                )?;
                for transformed in order {
                    query.probe_root(
                        if transformed { 22 } else { 11 },
                        if transformed {
                            WorldModelRegistrationKind::Transformed
                        } else {
                            WorldModelRegistrationKind::Static
                        },
                        &mut placement,
                    )?;
                }
                let scene = query.finish_scene().ok_or("missing overlapping roots")?;
                assert_eq!(scene.primary.owner, 11, "primary case {case}");
                assert_eq!(scene.primary.group, camera.group);
                assert_eq!(scene.primary.secondary_group, camera.secondary_group);
                let secondary = scene.secondary.ok_or("lost transformed camera root")?;
                assert_eq!(secondary.owner, 22, "secondary case {case}");
                assert_eq!(secondary.group, camera.group);
                assert_eq!(secondary.secondary_group, camera.secondary_group);
            }
        }
    }
    Ok(())
}
use solarity_systems::TerrainRegistrationPoint;
use std::error::Error;
use std::sync::Arc;

/// All arithmetic, tile admission and float outputs come from original code.
#[test]
fn submerged_world_model_matches_original_instruction_fixtures() -> Result<(), Box<dyn Error>> {
    let cases = include_str!("../fixtures/liquid-wmo-oracle.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            line.split_whitespace()
                .map(|word| u32::from_str_radix(word, 16))
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut files = Vec::new();
    let mut root = super::collision::root_fixture();
    root[80..82].copy_from_slice(&4u16.to_le_bytes());
    for (index, words) in cases.iter().enumerate() {
        let mut group = super::collision::group_fixture(8);
        group[28..32].copy_from_slice(&0x1000u32.to_le_bytes());
        group[72..76].copy_from_slice(&words[1].to_le_bytes());
        let mut grid = Vec::new();
        for word in [3, 3, 2, 2].iter().chain(&words[2..5]) {
            grid.extend(word.to_le_bytes());
        }
        grid.extend(0u16.to_le_bytes());
        for height in &words[8..17] {
            grid.extend(0u32.to_le_bytes());
            grid.extend(height.to_le_bytes());
        }
        grid.extend(words[17..21].iter().map(|word| *word as u8));
        let old_size = u32::from_le_bytes(group[16..20].try_into()?);
        group.extend(b"QILM");
        group.extend((grid.len() as u32).to_le_bytes());
        group.extend(&grid);
        group[16..20].copy_from_slice(&(old_size + 8 + grid.len() as u32).to_le_bytes());
        files.push((format!("World\\Wmo\\Liquid{index}.wmo"), root.clone()));
        files.push((format!("World\\Wmo\\Liquid{index}_000.wmo"), group));
    }
    let mut table = b"WDBC".to_vec();
    for word in [2u32, 45, 180, 1] {
        table.extend(word.to_le_bytes());
    }
    for (id, flags) in [(21u32, 0u32), (22, 4)] {
        let mut row = vec![0u8; 180];
        row[..4].copy_from_slice(&id.to_le_bytes());
        row[8..12].copy_from_slice(&flags.to_le_bytes());
        table.extend(row);
    }
    table.push(0);
    files.push(("DBFilesClient\\LiquidType.dbc".to_owned(), table));
    let fixture = Fixture::new(
        &files
            .iter()
            .map(|(path, bytes)| FixtureFile { path, bytes })
            .collect::<Vec<_>>(),
    )?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let liquids = LiquidTypeCatalog::load(&mut store)?;
    for (index, words) in cases.iter().enumerate() {
        let model = Arc::new(DecodedWorldModel::load(
            &mut store,
            &AssetPath::new(format!("World\\Wmo\\Liquid{index}.wmo"))?,
        )?);
        let translated = PlacedWorldModelCollision::prepare_transform(
            Arc::clone(&model),
            Mat4::from_translation(Vec3::new(0., 0., 32.)),
        )?;
        let placement = PlacedWorldModelCollision::prepare_transform(model, Mat4::IDENTITY)?;
        let point = Vec3::from_array(std::array::from_fn(|axis| f32::from_bits(words[5 + axis])));
        let sample = placement.registered_submerged_liquid(0, point, &liquids)?;
        assert_eq!(sample.is_some(), words[21] != 0, "admission case {index}");
        if let Some(sample) = sample {
            assert_eq!(sample.liquid_type, words[22], "type case {index}");
            assert_eq!(
                sample.surface_height.to_bits(),
                words[23],
                "height case {index}"
            );
            let unit = translated
                .registered_unit_liquid(0, point + Vec3::new(0., 0., 32.), &liquids)?
                .ok_or("translated unit liquid")?;
            assert_eq!(unit.liquid_type, sample.liquid_type);
            assert!((unit.surface_height - (sample.surface_height + 32.)).abs() < 0.00001);
            assert!((unit.depth - sample.depth).abs() < 0.00001);
        }
    }
    Ok(())
}

#[test]
fn submerged_terrain_uses_bilinear_height_floor_rejection_and_native_epsilon()
-> Result<(), Box<dyn Error>> {
    let mut adt = super::movement_collection::terrain_tile(32, 32)?;
    let mut liquid = vec![0u8; 3072 + 24];
    liquid[0..4].copy_from_slice(&3072u32.to_le_bytes());
    liquid[4..8].copy_from_slice(&1u32.to_le_bytes());
    liquid[3072..3074].copy_from_slice(&1u16.to_le_bytes());
    liquid[3080..3084].copy_from_slice(&8f32.to_le_bytes());
    liquid[3086..3088].copy_from_slice(&[1, 1]);
    liquid[3092..3096].copy_from_slice(&3096u32.to_le_bytes());
    for value in [0f32, 0., 0., 8.] {
        liquid.extend(value.to_le_bytes());
    }
    liquid.extend([255; 4]);
    adt.extend(b"O2HM");
    adt.extend((liquid.len() as u32).to_le_bytes());
    adt.extend(liquid);
    let map = super::movement_collection::map_table();
    let wdt = super::movement_collection::world_table()?;
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "DBFilesClient\\Map.dbc",
            bytes: &map,
        },
        FixtureFile {
            path: "World\\Maps\\Northrend\\Northrend.wdt",
            bytes: &wdt,
        },
        FixtureFile {
            path: "World\\Maps\\Northrend\\Northrend_32_32.adt",
            bytes: &adt,
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let catalog = MapCatalog::load(&mut store)?;
    let map = TerrainMap::load(&mut store, catalog.map(571).ok_or("fixture map")?)?;
    // These coordinates produce exactly 4096.5 in 7A0820's stored grid value.
    let point = TerrainRegistrationPoint::new(-2.084_365_8, -2.084_365_8)?;
    let tile = map.load_tile(&mut store, point.tile())?;
    let sample = point
        .submerged_liquid(&tile, 1.5, Some(-5.))?
        .ok_or("submerged sample")?;
    assert_eq!(sample.liquid_type, 1);
    assert_eq!(sample.surface_height, 2.);
    assert!(point.submerged_liquid(&tile, 2.005, Some(-5.))?.is_some());
    assert!(point.submerged_liquid(&tile, 2.02, Some(-5.))?.is_none());
    assert!(point.submerged_liquid(&tile, 1.5, Some(3.))?.is_none());
    assert_eq!(
        point
            .model_liquid(&tile, 1.5)?
            .map(|sample| sample.surface_height),
        Some(2.)
    );
    assert_eq!(
        point
            .model_liquid(&tile, -20_000.)?
            .map(|sample| sample.surface_height),
        Some(2.)
    );
    assert!(point.model_liquid(&tile, 2.02)?.is_none());
    assert!(point.submerged_liquid(&tile, 1.5, None)?.is_some());
    let outside = TerrainRegistrationPoint::new(-6., -6.)?;
    assert!(outside.submerged_liquid(&tile, 0., None)?.is_none());
    Ok(())
}
