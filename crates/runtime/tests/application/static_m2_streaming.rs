//! Static publication retains native placement lifetimes across source compaction.

use std::error::Error;
use std::sync::Arc;

use glam::Vec3;
use solarity_asset::{
    AnimationDataCatalog, ArchiveCatalog, AssetStore, AssetStoreHandle, ClientDataRoot, Locale,
    MapCatalog,
};
use solarity_ecs::{ActiveWorld, WorldBootstrap, WorldMapId};
use solarity_rendering::M2ParticleTwinkleTable;

use crate::application::terrain_coordinator::RuntimeTerrainCoordinator;
use crate::configuration::{WindowConfiguration, WindowMode};
use crate::platform::SdlPlatform;
use crate::test_support::{
    ClientFixture, SDL_TEST_LOCK, bootstrap_texture_blp, game_object_models,
};

use super::{CrtRand, M2Frame, M2GpuPlacementOwner, M2PlaybackStorage, ResidentM2Owner};

#[test]
fn static_owners_survive_overlap_and_remapped_sources_exclude_dynamic_materials()
-> Result<(), Box<dyn Error>> {
    let _guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let fixture = fixture()?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let animations = Arc::new(AnimationDataCatalog::load(&mut store)?);
    let maps = MapCatalog::load(&mut store)?;
    let mut terrain = RuntimeTerrainCoordinator::new(AssetStoreHandle::new(store), maps);
    terrain.synchronize(Some(&world(1000.)))?;
    let platform = SdlPlatform::start(WindowConfiguration::new(128, 128, WindowMode::Windowed))?;
    let mut renderer = super::game_object_scene_tests::renderer(&platform)?;
    let mut random = CrtRand::new();
    let mut frame = M2Frame::prepare(
        &mut renderer,
        terrain.resident_m2_scene().ok_or("initial scene")?,
        animations,
        &mut random,
        Arc::new(M2ParticleTwinkleTable::new(1)),
    )?;
    assert_eq!(owners(&frame), [10, 20]);
    assert_worker_spatial(&frame)?;
    assert_eq!(frame.sources.len(), 2);
    let retained_mesh = frame.sources[1].as_ref().ok_or("shared source")?.mesh;
    frame.placements[1].last_effect_time_ms = 777;
    frame.placements[1]
        .playback
        .as_mut()
        .map(M2PlaybackStorage::borrow_mut)
        .ok_or("shared playback")?
        .cycle_started_ms = 222.;

    // Build 12340's chunk references (0x007A50C0) keep the shared owner alive.
    // Removing its earlier source neighbor must update the compact source index.
    terrain.synchronize(Some(&world(500.)))?;
    let scene = terrain.resident_m2_scene().ok_or("second scene")?;
    let mut expected = random;
    roll_owners(&mut expected, 1);
    frame.synchronize_static_scenes(&mut renderer, [scene, scene].into_iter(), &mut random)?;
    assert_eq!(
        random, expected,
        "duplicate ADT references start only one new owner"
    );
    assert_eq!(owners(&frame), [20, 30]);
    assert_worker_spatial(&frame)?;
    assert_eq!(frame.sources.len(), 1);
    assert_eq!(frame.placements[0].source_index, 0);
    assert_eq!(
        frame.sources[0].as_ref().ok_or("remapped source")?.mesh,
        retained_mesh
    );
    assert_eq!(frame.placements[0].last_effect_time_ms, 777);
    assert_eq!(
        frame.placements[0]
            .playback
            .as_ref()
            .map(M2PlaybackStorage::borrow)
            .ok_or("retained playback")?
            .cycle_started_ms,
        222.
    );

    terrain.synchronize(Some(&world(-10.)))?;
    let scene = terrain.resident_m2_scene().ok_or("third scene")?;
    roll_owners(&mut expected, 2);
    frame.synchronize_static_scenes(&mut renderer, [scene].into_iter(), &mut random)?;
    assert_eq!(random, expected);
    assert_eq!(owners(&frame), [20, 40, 50]);
    assert_worker_spatial(&frame)?;
    assert_eq!(frame.sources.len(), 2);
    assert_eq!(
        frame
            .placements
            .iter()
            .map(|placement| placement.source_index)
            .collect::<Vec<_>>(),
        [0, 0, 1]
    );
    assert_eq!(frame.placements[0].last_effect_time_ms, 777);
    frame
        .placement_visibility
        .rebuild(&frame.placements, &frame.sources);
    frame.placement_topology_dirty = false;
    frame.synchronize_static_scenes(&mut renderer, [scene].into_iter(), &mut random)?;
    assert!(
        !frame.placement_topology_dirty,
        "unchanged publication keeps valid placement metadata"
    );
    assert_eq!(
        random, expected,
        "retained publication consumes no randomness"
    );
    frame.placement_topology_dirty = true;
    frame.synchronize_static_scenes(&mut renderer, [scene].into_iter(), &mut random)?;
    assert!(
        frame.placement_topology_dirty,
        "publication must preserve pending invalidation from another owner"
    );

    // A dynamic source may share the decoded model while owning different texture
    // replacements. Keep that source alive while every static reference leaves.
    let dynamic_source = frame.sources.len();
    frame.sources.push(frame.sources[0].clone());
    let mut dynamic = super::streaming::static_gpu_placement(
        &scene.placements()[0],
        dynamic_source,
        frame.sources[dynamic_source].as_ref(),
        &frame.animations,
        0.,
        &mut random,
    )?;
    dynamic.owner = M2GpuPlacementOwner::GluePet;
    frame.placements.push(dynamic);
    frame.synchronize_static_scenes(&mut renderer, std::iter::empty(), &mut random)?;
    assert_eq!(frame.placements.len(), 1);
    assert_eq!(frame.sources.len(), 1);
    assert_eq!(frame.placements[0].owner, M2GpuPlacementOwner::GluePet);
    assert_eq!(frame.placements[0].source_index, 0);
    expected = random;
    roll_owners(&mut expected, 3);
    frame.synchronize_static_scenes(&mut renderer, [scene].into_iter(), &mut random)?;
    assert_eq!(
        random, expected,
        "last-reference removal permits fresh playback on return"
    );
    assert_eq!(owners(&frame), [20, 40, 50]);
    assert_worker_spatial(&frame)?;
    assert_eq!(
        frame.sources.len(),
        3,
        "static materials do not reuse the dynamic source"
    );
    assert_eq!(
        frame
            .placements
            .iter()
            .map(|placement| placement.source_index)
            .collect::<Vec<_>>(),
        [0, 1, 1, 2]
    );

    // Empty geometry is an occupied source while a live owner references it.
    // Retirement must preserve it even when all renderable scenery leaves.
    let empty_source = frame.sources.len();
    frame.sources.push(None);
    let mut empty = super::streaming::static_gpu_placement(
        &scene.placements()[0],
        empty_source,
        None,
        &frame.animations,
        0.,
        &mut random,
    )?;
    empty.owner = M2GpuPlacementOwner::GluePet;
    frame.placements.push(empty);
    frame.synchronize_static_scenes(&mut renderer, std::iter::empty(), &mut random)?;
    assert_eq!(frame.placements.len(), 2);
    assert_eq!(frame.sources.len(), 2);
    assert_eq!(frame.placements[1].source_index, 1);
    assert!(frame.sources[1].is_none());
    // No static owner retires in this publication. Source liveness must still
    // retain the dynamic empty mesh and discard an unreferenced prepared slot.
    frame
        .placement_visibility
        .rebuild(&frame.placements, &frame.sources);
    frame.placement_topology_dirty = false;
    frame.sources.push(None);
    frame.synchronize_static_scenes(&mut renderer, std::iter::empty(), &mut random)?;
    assert_eq!(frame.placements.len(), 2);
    assert_eq!(frame.sources.len(), 2);
    assert_eq!(frame.placements[1].source_index, 1);
    assert!(frame.sources[1].is_none());
    assert!(frame.placement_topology_dirty);

    // A hole before live slots rebases their indices. A later publication must
    // use the live records until the compact metadata has been rebuilt.
    frame.sources.insert(0, None);
    for placement in &mut frame.placements {
        placement.source_index += 1;
    }
    frame
        .placement_visibility
        .rebuild(&frame.placements, &frame.sources);
    frame.placement_topology_dirty = false;
    frame.compact_sources();
    assert!(frame.placement_topology_dirty);
    frame.synchronize_static_scenes(&mut renderer, std::iter::empty(), &mut random)?;
    assert_eq!(frame.placements.len(), 2);
    assert_eq!(frame.sources.len(), 2);
    assert_eq!(frame.placements[1].source_index, 1);
    assert!(frame.sources[1].is_none());
    frame.placements.clear();
    frame.compact_sources();
    assert!(frame.sources.is_empty());
    assert_eq!(random, expected, "retirement consumes no randomness");
    Ok(())
}

/// Exercises scene generations through coordinator admission and real GPU publication.
#[test]
fn scene_generation_changes_preserve_shared_owner_clocks_and_publication_order()
-> Result<(), Box<dyn Error>> {
    let _guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let fixture = fixture()?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let animations = Arc::new(AnimationDataCatalog::load(&mut store)?);
    let maps = MapCatalog::load(&mut store)?;
    let mut terrain = RuntimeTerrainCoordinator::new(AssetStoreHandle::new(store), maps);
    terrain.synchronize(Some(&world(1000.)))?;
    let first = Arc::clone(terrain.resident_m2_scene().ok_or("initial scene")?);
    let platform = SdlPlatform::start(WindowConfiguration::new(128, 128, WindowMode::Windowed))?;
    let mut renderer = super::game_object_scene_tests::renderer(&platform)?;
    let mut random = CrtRand::new();
    let mut frame = M2Frame::prepare(
        &mut renderer,
        &first,
        animations,
        &mut random,
        Arc::new(M2ParticleTwinkleTable::new(1)),
    )?;
    frame.placements[1].last_effect_time_ms = 777;
    let mut expected = random;
    frame.synchronize_static_scenes(&mut renderer, [&first, &first].into_iter(), &mut random)?;
    assert_eq!(random, expected, "initial publication adds no extra owner");
    assert!(
        frame.placement_topology_dirty,
        "initial topology still needs preparation"
    );

    terrain.synchronize(Some(&world(500.)))?;
    let second = Arc::clone(terrain.resident_m2_scene().ok_or("second scene")?);
    roll_owners(&mut expected, 1);
    frame.synchronize_static_scenes(
        &mut renderer,
        [&second, &first, &second].into_iter(),
        &mut random,
    )?;
    assert_eq!(owners(&frame), [10, 20, 30]);
    assert_worker_spatial(&frame)?;
    assert_eq!(random, expected, "two scene references share owner 20");
    frame.synchronize_static_scenes(&mut renderer, [&second].into_iter(), &mut random)?;
    assert_eq!(owners(&frame), [20, 30]);
    assert_worker_spatial(&frame)?;
    assert_eq!(frame.placements[0].last_effect_time_ms, 777);
    assert_eq!(
        random, expected,
        "departure preserves the surviving reference"
    );

    // Rebuilding an ADT creates a new immutable generation, even at the same
    // map coordinates. Stock 0x007A50C0's overlapping references preserve the
    // already live owner when old and new generations exchange in one publish.
    terrain.synchronize(None)?;
    terrain.synchronize(Some(&world(500.)))?;
    let reloaded = Arc::clone(terrain.resident_m2_scene().ok_or("reloaded scene")?);
    assert!(!Arc::ptr_eq(&second, &reloaded));
    frame
        .placement_visibility
        .rebuild(&frame.placements, &frame.sources);
    frame.placement_topology_dirty = false;
    let previous_bounds = frame.placement_visibility.bounds().to_vec();
    frame.synchronize_static_scenes(&mut renderer, [&reloaded].into_iter(), &mut random)?;
    assert!(
        !frame.placement_topology_dirty,
        "a scene generation exchange retains unchanged owner topology"
    );
    assert_eq!(frame.placement_visibility.bounds(), previous_bounds);
    let mut referenced_sources = vec![usize::MAX; frame.sources.len()];
    frame
        .placement_visibility
        .mark_source_references(&mut referenced_sources);
    assert!(
        frame
            .placements
            .iter()
            .all(|placement| referenced_sources[placement.source_index] == 0)
    );
    assert_eq!(owners(&frame), [20, 30]);
    assert_worker_spatial(&frame)?;
    assert_eq!(frame.placements[0].last_effect_time_ms, 777);
    assert_eq!(
        random, expected,
        "a shared owner outlives its scene generation"
    );

    // Retaining CPU generation handles alone must not keep any GPU owner alive.
    frame.synchronize_static_scenes(&mut renderer, std::iter::empty(), &mut random)?;
    assert!(frame.placements.is_empty());
    assert!(frame.sources.is_empty());
    assert!(
        frame.placement_topology_dirty,
        "last-reference retirement invalidates topology"
    );
    roll_owners(&mut expected, 3);
    frame.synchronize_static_scenes(&mut renderer, [&second, &first].into_iter(), &mut random)?;
    assert_eq!(
        owners(&frame),
        [20, 30, 10],
        "new owners follow input scene order"
    );
    assert_eq!(
        random, expected,
        "returning owners start fresh exactly once"
    );
    Ok(())
}

/// Reads the externally authored order while allowing the test's live dynamic owner.
fn owners(frame: &M2Frame) -> Vec<u32> {
    frame
        .placements
        .iter()
        .filter_map(|placement| match placement.owner {
            M2GpuPlacementOwner::Static(ResidentM2Owner::TerrainDoodad { unique_id }) => {
                Some(unique_id)
            }
            _ => None,
        })
        .collect()
}

/// Stock default Stand selection consumes a variation roll and a cycle roll.
fn roll_owners(random: &mut CrtRand, count: usize) {
    for _ in 0..count * 2 {
        let _ = random.next_u15();
    }
}

/// Selects consecutive ADTs through the production terrain coordinator.
fn world(x: f32) -> ActiveWorld {
    ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        1,
        "StaticFixture",
        Vec3::new(x, 5800., 10.),
        0.,
    ))
}

/// Authors three generations whose overlapping identities exercise both source slots.
fn fixture() -> Result<ClientFixture, Box<dyn Error>> {
    let mut map = [0_u32; 66];
    map[0] = 571;
    map[1] = 1;
    map[5] = 1;
    map[22] = 571;
    map[59] = u32::MAX;
    map[63] = 2;
    let mut dbc = b"WDBC".to_vec();
    for value in [1_u32, 66, 264, 11] {
        dbc.extend_from_slice(&value.to_le_bytes());
    }
    for value in map {
        dbc.extend_from_slice(&value.to_le_bytes());
    }
    dbc.extend_from_slice(b"\0Northrend\0");
    let mut manifest = wow_wdt::WdtFile::new(wow_wdt::version::WowVersion::WotLK);
    manifest.mwmo = Some(wow_wdt::chunks::MwmoChunk::new());
    for y in 30..=32 {
        manifest
            .main
            .get_mut(21, y)
            .ok_or("tile")?
            .set_has_adt(true);
    }
    let mut wdt = Vec::new();
    wow_wdt::WdtWriter::new(&mut wdt).write(&manifest)?;
    let first = adt(&[(10, 0), (20, 1)])?;
    let second = adt(&[(20, 1), (30, 1)])?;
    let third = adt(&[(20, 1), (40, 1), (50, 0)])?;
    let model = game_object_models::model_with_animations(&[0])?;
    let skin = game_object_models::skin()?;
    ClientFixture::with_common_files(&[
        ("DBFilesClient\\Map.dbc", &dbc),
        ("World\\Maps\\Northrend\\Northrend.wdt", &wdt),
        ("World\\Maps\\Northrend\\Northrend_21_30.adt", &first),
        ("World\\Maps\\Northrend\\Northrend_21_31.adt", &second),
        ("World\\Maps\\Northrend\\Northrend_21_32.adt", &third),
        ("tileset\\fixture\\grass.blp", &bootstrap_texture_blp()),
        ("World\\Old.m2", &model),
        ("World\\Old00.skin", &skin),
        ("World\\Shared.m2", &model),
        ("World\\Shared00.skin", &skin),
    ])
}

/// Publishes explicit MCNK references so MDDF declarations become resident owners.
fn adt(owners: &[(u32, u32)]) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut builder = wow_adt::builder::AdtBuilder::new()
        .with_version(wow_adt::AdtVersion::WotLK)
        .add_texture("tileset/fixture/grass.blp")
        .add_model("World/Old.m2")
        .add_model("World/Shared.m2");
    for &(unique_id, name_id) in owners {
        builder = builder.add_doodad_placement(wow_adt::DoodadPlacement {
            name_id,
            unique_id,
            position: [17066., 0., 17066.],
            rotation: [0.; 3],
            scale: 1024,
            flags: 0,
        });
    }
    let mut bytes = builder.build()?.to_bytes()?;
    let chunk = bytes
        .windows(4)
        .rposition(|word| word == b"KNCM")
        .ok_or("MCNK")?;
    let size = u32::from_le_bytes(bytes[chunk + 4..chunk + 8].try_into()?) as usize;
    let end = chunk + 8 + size;
    set_u32(&mut bytes, chunk + 8 + 0x20, (8 + size) as u32);
    set_u32(&mut bytes, chunk + 8 + 0x10, owners.len() as u32);
    let mut references = b"FRCM".to_vec();
    references.extend_from_slice(&(owners.len() as u32 * 4).to_le_bytes());
    for index in 0..owners.len() as u32 {
        references.extend_from_slice(&index.to_le_bytes());
    }
    let final_size = size + references.len();
    bytes.splice(end..end, references);
    set_u32(&mut bytes, chunk + 4, final_size as u32);
    let mcin = bytes
        .windows(4)
        .position(|word| word == b"NICM")
        .ok_or("MCIN")?;
    set_u32(&mut bytes, mcin + 8 + 255 * 16 + 4, final_size as u32);
    Ok(bytes)
}

fn set_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

/// Compare the worker payload with the original presentation-time sphere
/// calculation after owner retention and source/placement slot compaction.
fn assert_worker_spatial(frame: &M2Frame) -> Result<(), Box<dyn Error>> {
    for placement in &frame.placements {
        if !matches!(placement.owner, M2GpuPlacementOwner::Static(_)) {
            continue;
        }
        let Some(source) = &frame.sources[placement.source_index] else {
            assert!(placement.static_spatial.is_none());
            continue;
        };
        let spatial = placement
            .static_spatial
            .ok_or("missing worker spatial data")?;
        let expected = super::placement_bounding_sphere(&source.model, placement.transform);
        assert_eq!(
            spatial.sphere().0.to_array().map(f32::to_bits),
            expected.0.to_array().map(f32::to_bits)
        );
        assert_eq!(spatial.sphere().1.to_bits(), expected.1.to_bits());
        let bounds = source.model.bounds();
        let reference = super::distance::SceneryDistance::new(
            bounds.minimum(),
            bounds.maximum(),
            placement.transform,
        );
        for detail in [0.5, 1., 1.5] {
            for depth in [0., 30., 100., 200., 750., 1250.] {
                let camera = placement.transform.w_axis.truncate() + Vec3::X * depth;
                assert_eq!(
                    spatial.scenery().opacity(camera, detail).to_bits(),
                    reference.opacity(camera, detail).to_bits()
                );
                assert_eq!(
                    spatial.scenery().admits_shadow(camera, detail),
                    reference.admits_shadow(camera, detail)
                );
                assert_eq!(
                    spatial.scenery().admits_group(depth, detail),
                    reference.admits_group(depth, detail)
                );
            }
        }
    }
    Ok(())
}
