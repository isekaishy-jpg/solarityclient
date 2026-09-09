//! External integration tests for active-world terrain residency.

#[path = "terrain/dynamic_movement.rs"]
mod dynamic_movement;

use std::error::Error;
use std::io::Cursor;
use std::num::NonZeroUsize;

use glam::Vec3;
use solarity_asset::{
    AnimationDataCatalog, ArchiveCatalog, AssetStore, AssetStoreHandle, CharacterAppearanceCatalog,
    CharacterRaceCatalog, CharacterStartOutfitCatalog, ClientDataRoot, CreatureCatalog,
    CreatureFamilyCatalog, DecodedTerrainTile, HelmetGeosetVisibilityCatalog,
    ItemDefinitionCatalog, ItemDisplayCatalog, ItemVisualCatalog, Locale, MapCatalog,
    ParticleColorCatalog, TerrainTileIndex,
};
use solarity_cpu::{CpuExecutor, CpuPoolConfig};
use solarity_ecs::{ActiveWorld, PlayerViewState, WorldBootstrap, WorldMapId, WorldTransform};
use solarity_rendering::{
    CharacterComponentTextureLevel, WorldCamera, WorldFrustum, WorldScreenWindow,
};
use solarity_runtime::{
    RuntimeCreaturePoll, RuntimeMovementReference, RuntimeMovementRegistrationQuery,
    RuntimePlayerCatalogs, RuntimePlayerItemCatalogs, RuntimePlayerPoll, RuntimePlayerPresentation,
    RuntimeRemotePlayerPoll, RuntimeStaticMovementOwner, RuntimeStaticMovementQuery,
    RuntimeStaticMovementResidency, RuntimeTerrainCoordinator, RuntimeTerrainError,
    RuntimeTerrainPoll, RuntimeTerrainStreamPoll,
};
use solarity_systems::{
    CameraSubjectGeometry, MovementBspCacheMode, MovementCollisionBounds, PlacedM2Collision,
    TerrainStreamingWindow, WorldViewDistanceRequest, project_object_fields,
    resolve_camera_subject_height, resolve_player_camera_pose, resolve_world_view_distance,
};
use wow_adt::AdtVersion;
use wow_adt::builder::AdtBuilder;
use wow_adt::{DoodadPlacement, WmoPlacement};
use wow_m2::chunks::texture::{M2Texture as RawTexture, M2TextureFlags, M2TextureType};
use wow_m2::chunks::vertex::M2Vertex;
use wow_m2::common::{C2Vector, C3Vector, FixedString, M2Array, M2ArrayString};
use wow_m2::header::M2Header;
use wow_m2::skin::{OldSkinHeader, SkinSubmesh};
use wow_m2::{M2Model, M2Version, OldSkin};
use wow_wdt::chunks::{ModfChunk, ModfEntry, MphdFlags, MwmoChunk};
use wow_wdt::version::WowVersion;
use wow_wdt::{WdtFile, WdtWriter};

use crate::support::{ClientFixture, bootstrap_texture_blp};

#[test]
fn game_object_registration_links_resident_destinations_and_skips_missing_tiles()
-> Result<(), Box<dyn Error>> {
    let first = TerrainTileIndex::new(21, 30).ok_or("bad tile")?;
    let second = TerrainTileIndex::new(22, 30).ok_or("bad tile")?;
    let mut manifest = WdtFile::new(WowVersion::WotLK);
    manifest.mwmo = Some(MwmoChunk::new());
    for tile in [first, second] {
        manifest
            .main
            .get_mut(usize::from(tile.x()), usize::from(tile.y()))
            .ok_or("bad tile")?
            .set_has_adt(true);
    }
    let mut wdt = Vec::new();
    WdtWriter::new(&mut wdt).write(&manifest)?;
    let fixture = ClientFixture::with_common_files(&[
        ("DBFilesClient\\Map.dbc", &map_table()),
        ("World\\Maps\\Northrend\\Northrend.wdt", &wdt),
        (
            "World\\Maps\\Northrend\\Northrend_21_30.adt",
            &streaming_adt(first, 10.)?,
        ),
        ("tileset\\fixture\\grass.blp", &bootstrap_texture_blp()),
        ("World\\Fixture\\Collision.m2", &m2_collision_fixture()?),
        ("World\\Fixture\\Collision00.skin", &skin_fixture()?),
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let maps = MapCatalog::load(&mut store)?;
    let model = std::sync::Arc::new(solarity_asset::DecodedM2Model::load(
        &mut store,
        &solarity_asset::AssetPath::new("World\\Fixture\\Collision.m2")?,
    )?);
    let mut placed = PlacedM2Collision::prepare_transform(
        model,
        glam::Mat4::from_translation(Vec3::new(999., 5799., 50.)),
    )?;
    let mut terrain = RuntimeTerrainCoordinator::new(AssetStoreHandle::new(store), maps);
    let mut query = RuntimeMovementRegistrationQuery::new();
    assert_eq!(
        terrain.register_game_object_movement(
            571,
            &placed,
            MovementBspCacheMode::Enabled,
            &mut query
        )?,
        RuntimeStaticMovementResidency::PendingMap
    );
    let world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        1,
        "RegistrationFixture",
        Vec3::new(1000., 5800., 50.),
        0.,
    ));
    terrain.synchronize(Some(&world))?;
    assert_eq!(
        terrain.register_game_object_movement(
            571,
            &placed,
            MovementBspCacheMode::Enabled,
            &mut query
        )?,
        RuntimeStaticMovementResidency::Ready
    );
    assert_eq!(query.map_id(), Some(571));
    assert!(!query.references().is_empty());
    assert!(query.references().iter().all(|reference|
        matches!(reference, RuntimeMovementReference::Terrain { tile, .. } if *tile == first)));
    assert!(
        query
            .selection()
            .ok_or("missing complete selection")?
            .selected()
            .is_none()
    );
    placed.set_transform(glam::Mat4::from_translation(Vec3::new(999., 5299., 50.)))?;
    assert_eq!(
        terrain.register_game_object_movement(
            571,
            &placed,
            MovementBspCacheMode::Enabled,
            &mut query
        )?,
        RuntimeStaticMovementResidency::Ready
    );
    assert_eq!(query.map_id(), Some(571));
    assert!(query.references().is_empty());
    assert!(query.selection().is_some());
    // 7C2040 links resident chunks even when the render bounds also cover an
    // unloaded tile. The query's own terrain admission remains independent.
    placed.set_transform(glam::Mat4::from_translation(Vec3::new(999., 5333.5, 50.)))?;
    assert_eq!(
        terrain.register_game_object_movement(
            571,
            &placed,
            MovementBspCacheMode::Enabled,
            &mut query
        )?,
        RuntimeStaticMovementResidency::Ready
    );
    assert!(!query.references().is_empty());
    assert!(query.references().iter().all(|reference|
        matches!(reference, RuntimeMovementReference::Terrain { tile, .. } if *tile == first)));
    placed.set_transform(glam::Mat4::from_translation(Vec3::new(999., 5799., -20.)))?;
    assert_eq!(
        terrain.register_game_object_movement(
            571,
            &placed,
            MovementBspCacheMode::Enabled,
            &mut query
        )?,
        RuntimeStaticMovementResidency::Ready
    );
    assert!(
        query.references().is_empty(),
        "native chunk minimum Z rejects a buried model"
    );
    assert_eq!(
        terrain.register_game_object_movement(
            0,
            &placed,
            MovementBspCacheMode::Enabled,
            &mut query
        )?,
        RuntimeStaticMovementResidency::PendingMap
    );
    assert_eq!(query.map_id(), None);
    Ok(())
}

/// Neighbor preparation survives ordinary movement, but never a retired world.
#[test]
fn terrain_streaming_retains_neighbors_and_retires_old_jobs() -> Result<(), Box<dyn Error>> {
    let first = TerrainTileIndex::new(21, 30).ok_or("bad first tile")?;
    let second = TerrainTileIndex::new(22, 30).ok_or("bad second tile")?;
    let mut manifest = WdtFile::new(WowVersion::WotLK);
    manifest.mwmo = Some(MwmoChunk::new());
    for tile in [first, second] {
        manifest
            .main
            .get_mut(usize::from(tile.x()), usize::from(tile.y()))
            .ok_or("bad tile")?
            .set_has_adt(true);
    }
    let mut wdt = Vec::new();
    WdtWriter::new(&mut wdt).write(&manifest)?;
    let fixture = ClientFixture::with_common_files(&[
        ("DBFilesClient\\Map.dbc", &map_table()),
        ("World\\Maps\\Northrend\\Northrend.wdt", &wdt),
        (
            "World\\Maps\\Northrend\\Northrend_21_30.adt",
            &streaming_adt(first, 10.)?,
        ),
        (
            "World\\Maps\\Northrend\\Northrend_22_30.adt",
            &streaming_adt(second, 50.)?,
        ),
        ("tileset\\fixture\\grass.blp", &bootstrap_texture_blp()),
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog.clone())?;
    let maps = MapCatalog::load(&mut store)?;
    let mut terrain = RuntimeTerrainCoordinator::new(AssetStoreHandle::new(store), maps)
        .with_worker_catalog(catalog);
    let origin = Vec3::new(1000., 5800., 250.);
    let world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        1,
        "StreamingFixture",
        origin,
        0.,
    ));
    let next_world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        1,
        "StreamingFixture",
        Vec3::new(1000., 5300., 250.),
        0.,
    ));
    terrain.synchronize(Some(&world))?;
    let movement_bounds =
        MovementCollisionBounds::new(Vec3::new(999., 5332., -1.), Vec3::new(1001., 5334., 60.))?;
    let mut movement = RuntimeStaticMovementQuery::new();
    assert_eq!(
        terrain.collect_static_movement(
            571,
            movement_bounds,
            MovementBspCacheMode::Enabled,
            &mut movement
        )?,
        RuntimeStaticMovementResidency::PendingTile { tile: second }
    );
    assert!(movement.triangles().is_empty());
    assert_eq!(movement.map_id(), None);
    let distance = resolve_world_view_distance(WorldViewDistanceRequest::new(
        777.,
        WorldMapId::new(571),
        0x8000_0000,
    ))?;
    let window = TerrainStreamingWindow::new(origin, distance, Vec3::new(777., 0., 0.))?;
    assert!(window.contains(second));
    let cpu = CpuExecutor::new(CpuPoolConfig::new(
        NonZeroUsize::MIN,
        NonZeroUsize::new(3).ok_or("bad capacity")?,
    ))?;
    let (release, wait) = std::sync::mpsc::channel();
    let blocker = cpu.try_submit(move || wait.recv())?;
    assert_eq!(
        terrain.synchronize_streaming_async(571, origin, window, &cpu)?,
        RuntimeTerrainStreamPoll::Pending { remaining_tiles: 1 }
    );
    assert!(terrain.resident_tile_at(second).is_none());
    assert_eq!(terrain.resident_tile_count(), 1);
    assert_eq!(
        terrain.synchronize_async(Some(&world), &cpu)?,
        RuntimeTerrainPoll::Current {
            map_id: 571,
            tile: first
        }
    );
    terrain.disconnect();
    terrain.synchronize(Some(&world))?;
    release.send(())?;
    blocker.join()??;
    cpu.try_submit(|| ())?.join()?;
    // The completed old-world job must be discarded, then prepared under the
    // replacement world even though its map and tile identifiers are identical.
    assert_eq!(
        terrain.synchronize_streaming_async(571, origin, window, &cpu)?,
        RuntimeTerrainStreamPoll::Pending { remaining_tiles: 1 }
    );
    assert!(terrain.resident_tile_at(second).is_none());
    cpu.try_submit(|| ())?.join()?;
    assert_eq!(
        terrain.synchronize_streaming_async(571, origin, window, &cpu)?,
        RuntimeTerrainStreamPoll::Current
    );
    assert_eq!(terrain.resident_tile_count(), 2);
    assert_eq!(
        terrain.collect_static_movement(
            571,
            movement_bounds,
            MovementBspCacheMode::Enabled,
            &mut movement
        )?,
        RuntimeStaticMovementResidency::Ready
    );
    assert_eq!(movement.map_id(), Some(571));
    let movement_before_promotion = movement.triangles().to_vec();
    let movement_owners: Vec<_> = (0..movement.triangles().len())
        .map(|index| movement.owner(index))
        .collect();
    let mut seen_second = false;
    let mut seen_first = false;
    let native_chunks: Vec<_> = movement_bounds.terrain_chunks()?.collect();
    let mut previous_chunk = 0;
    for owner in &movement_owners {
        let Some(RuntimeStaticMovementOwner::Terrain { tile, chunk }) = owner else {
            return Err("unexpected static owner".into());
        };
        let ordinal = native_chunks
            .iter()
            .position(|address| address == &(*tile, *chunk))
            .ok_or("unselected chunk")?;
        assert!(ordinal >= previous_chunk, "query reordered terrain chunks");
        previous_chunk = ordinal;
        if *tile == first {
            seen_first = true;
        } else {
            assert_eq!(*tile, second);
            seen_second = true;
        }
    }
    assert!(seen_first && seen_second);
    let neighbor_point = Vec3::new(1000., 5300., 300.);
    let neighbor_height = terrain
        .controlled_player_terrain_height(neighbor_point)?
        .ok_or("neighbor height unavailable")?;
    assert!((neighbor_height - 50.).abs() < 0.00001);
    let contact = terrain
        .trace_collision(neighbor_point, neighbor_point - Vec3::Z * 300., 0., 1.)?
        .ok_or("neighbor terrain missed camera trace")?;
    assert!((contact.fraction() - 250. / 300.).abs() < 0.00001);
    // A busy worker proves the boundary crossing reuses the admitted neighbor.
    let (release, wait) = std::sync::mpsc::channel();
    let blocker = cpu.try_submit(move || wait.recv())?;
    let promoted = terrain.synchronize_async(Some(&next_world), &cpu);
    release.send(())?;
    blocker.join()??;
    assert_eq!(
        promoted?,
        RuntimeTerrainPoll::TileLoaded {
            map_id: 571,
            tile: second
        }
    );
    assert_eq!(terrain.resident_tile_count(), 2);
    assert_eq!(
        terrain.resident_tile().map(DecodedTerrainTile::index),
        Some(second)
    );
    assert_eq!(
        terrain.collect_static_movement(
            571,
            movement_bounds,
            MovementBspCacheMode::Enabled,
            &mut movement
        )?,
        RuntimeStaticMovementResidency::Ready
    );
    assert_eq!(movement.triangles().len(), movement_before_promotion.len());
    for (current, previous) in movement.triangles().iter().zip(&movement_before_promotion) {
        assert_eq!(current.vertices(), previous.vertices());
        assert_eq!(current.normal(), previous.normal());
    }
    assert_eq!(
        (0..movement.triangles().len())
            .map(|index| movement.owner(index))
            .collect::<Vec<_>>(),
        movement_owners
    );
    // An unrelated active map cannot inherit the previous query's geometry.
    assert_eq!(
        terrain.collect_static_movement(
            0,
            movement_bounds,
            MovementBspCacheMode::Enabled,
            &mut movement
        )?,
        RuntimeStaticMovementResidency::PendingMap
    );
    assert!(movement.triangles().is_empty());
    assert_eq!(movement.owner(0), None);
    let hole =
        MovementCollisionBounds::new(Vec3::new(999., 4500., -1.), Vec3::new(1001., 4501., 60.))?;
    assert_eq!(
        terrain.collect_static_movement(571, hole, MovementBspCacheMode::Enabled, &mut movement)?,
        RuntimeStaticMovementResidency::Ready
    );
    assert!(movement.triangles().is_empty());
    assert_eq!(movement.map_id(), Some(571));
    let distant_origin = Vec3::new(1000., 5050., 250.);
    let short_distance = resolve_world_view_distance(WorldViewDistanceRequest::new(
        183.333_33,
        WorldMapId::new(571),
        0x8000_0000,
    ))?;
    let smaller = TerrainStreamingWindow::new(
        distant_origin,
        short_distance,
        Vec3::new(short_distance.value(), 0., 0.),
    )?;
    assert!(!smaller.contains(first));
    assert_eq!(
        terrain.synchronize_streaming_async(571, distant_origin, smaller, &cpu)?,
        RuntimeTerrainStreamPoll::Current
    );
    assert_eq!(terrain.resident_tile_count(), 1);
    assert!(terrain.resident_tile_at(first).is_none());
    assert_eq!(
        terrain.collect_static_movement(
            571,
            movement_bounds,
            MovementBspCacheMode::Enabled,
            &mut movement
        )?,
        RuntimeStaticMovementResidency::PendingTile { tile: first }
    );
    assert!(movement.triangles().is_empty());
    Ok(())
}

/// Neighbor references share IDs only when their selected authored records agree.
#[test]
fn terrain_streaming_validates_shared_placement_identities() -> Result<(), Box<dyn Error>> {
    let first = TerrainTileIndex::new(21, 30).ok_or("bad first tile")?;
    let second = TerrainTileIndex::new(22, 30).ok_or("bad second tile")?;
    let origin = Vec3::new(1000., 5800., 250.);
    let distance = resolve_world_view_distance(WorldViewDistanceRequest::new(
        777.,
        WorldMapId::new(571),
        0x8000_0000,
    ))?;
    let window = TerrainStreamingWindow::new(origin, distance, Vec3::new(777., 0., 0.))?;
    // 0: identical owners, 1: MDDF conflict, 2: MODF conflict. An unselected
    // duplicate ID has different fields in every case and creates no owner.
    for conflict in 0..3 {
        let mut manifest = WdtFile::new(WowVersion::WotLK);
        manifest.mwmo = Some(MwmoChunk::new());
        for tile in [first, second] {
            manifest
                .main
                .get_mut(usize::from(tile.x()), usize::from(tile.y()))
                .ok_or("bad tile")?
                .set_has_adt(true);
        }
        let mut wdt = Vec::new();
        WdtWriter::new(&mut wdt).write(&manifest)?;
        let fixture = world_model_client_fixture(&[
            ("DBFilesClient\\Map.dbc", &map_table()),
            ("World\\Maps\\Northrend\\Northrend.wdt", &wdt),
            (
                "World\\Maps\\Northrend\\Northrend_21_30.adt",
                &streaming_placement_adt(first, 0)?,
            ),
            (
                "World\\Maps\\Northrend\\Northrend_22_30.adt",
                &streaming_placement_adt(second, conflict)?,
            ),
            ("tileset\\fixture\\grass.blp", &bootstrap_texture_blp()),
            ("World\\Wmo\\Fixture.wmo", &root_wmo_fixture()),
            (
                "World\\Wmo\\Fixture_000.wmo",
                &movement_reference_group_wmo()?,
            ),
            ("World\\Fixture\\Collision.m2", &m2_collision_fixture()?),
            ("World\\Fixture\\Collision00.skin", &skin_fixture()?),
            ("World\\Fixture\\Collision.blp", &bootstrap_texture_blp()),
        ])?;
        let catalog =
            ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
        let mut store = AssetStore::mount(catalog.clone())?;
        let maps = MapCatalog::load(&mut store)?;
        let mut terrain = RuntimeTerrainCoordinator::new(AssetStoreHandle::new(store), maps)
            .with_worker_catalog(catalog);
        let world = ActiveWorld::enter(WorldBootstrap::new(
            WorldMapId::new(571),
            1,
            "SharedPlacements",
            origin,
            0.,
        ));
        terrain.synchronize(Some(&world))?;
        let one = NonZeroUsize::new(1).ok_or("bad worker count")?;
        let cpu = CpuExecutor::new(CpuPoolConfig::new(
            one,
            NonZeroUsize::new(2).ok_or("bad worker capacity")?,
        ))?;
        assert_eq!(
            terrain.synchronize_streaming_async(571, origin, window, &cpu)?,
            RuntimeTerrainStreamPoll::Pending { remaining_tiles: 1 }
        );
        cpu.try_submit(|| ())?.join()?;
        let result = terrain.synchronize_streaming_async(571, origin, window, &cpu);
        match conflict {
            0 => {
                assert_eq!(result?, RuntimeTerrainStreamPoll::Current);
                assert_eq!(terrain.resident_tile_count(), 2);
                let next_world = ActiveWorld::enter(WorldBootstrap::new(
                    WorldMapId::new(571),
                    1,
                    "SharedPlacements",
                    Vec3::new(1000., 5300., 250.),
                    0.,
                ));
                assert_eq!(
                    terrain.synchronize_async(Some(&next_world), &cpu)?,
                    RuntimeTerrainPoll::TileLoaded {
                        map_id: 571,
                        tile: second
                    }
                );
                assert_eq!(terrain.resident_world_model_count(), 1);
                assert_eq!(terrain.resident_m2_count(), 2);
            }
            1 => assert!(matches!(
                result,
                Err(RuntimeTerrainError::ConflictingDoodadPlacement { unique_id: 9 })
            )),
            2 => assert!(matches!(
                result,
                Err(RuntimeTerrainError::ConflictingWorldModelPlacement { unique_id: 7 })
            )),
            _ => unreachable!(),
        }
        if conflict != 0 {
            assert!(terrain.resident_tile_at(second).is_none());
            assert_eq!(terrain.resident_tile_count(), 1);
        }
    }
    Ok(())
}

/// MCRF/MODR reference order and first visits survive a shared-ADT promotion.
#[test]
fn static_movement_retains_reference_order_and_placement_owners() -> Result<(), Box<dyn Error>> {
    let first = TerrainTileIndex::new(31, 31).ok_or("bad first tile")?;
    let second = TerrainTileIndex::new(32, 31).ok_or("bad second tile")?;
    let mut manifest = WdtFile::new(WowVersion::WotLK);
    manifest.mwmo = Some(MwmoChunk::new());
    for tile in [first, second] {
        manifest
            .main
            .get_mut(usize::from(tile.x()), usize::from(tile.y()))
            .ok_or("bad tile")?
            .set_has_adt(true);
    }
    let mut wdt = Vec::new();
    WdtWriter::new(&mut wdt).write(&manifest)?;
    let fixture = world_model_client_fixture(&[
        ("DBFilesClient\\Map.dbc", &map_table()),
        ("World\\Maps\\Northrend\\Northrend.wdt", &wdt),
        (
            "World\\Maps\\Northrend\\Northrend_31_31.adt",
            &movement_reference_adt(first, false)?,
        ),
        (
            "World\\Maps\\Northrend\\Northrend_32_31.adt",
            &movement_reference_adt(second, true)?,
        ),
        ("tileset\\fixture\\grass.blp", &bootstrap_texture_blp()),
        ("World\\Wmo\\Fixture.wmo", &movement_reference_root_wmo()?),
        (
            "World\\Wmo\\Fixture_000.wmo",
            &movement_reference_group_wmo()?,
        ),
        (
            "World\\Wmo\\Fixture_001.wmo",
            &movement_reference_group_wmo()?,
        ),
        ("World\\Fixture\\Collision.m2", &m2_collision_fixture()?),
        ("World\\Fixture\\Collision00.skin", &skin_fixture()?),
        ("World\\Fixture\\Collision.blp", &bootstrap_texture_blp()),
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog.clone())?;
    let maps = MapCatalog::load(&mut store)?;
    let mut terrain = RuntimeTerrainCoordinator::new(AssetStoreHandle::new(store), maps)
        .with_worker_catalog(catalog);
    let origin = Vec3::new(1., 1., 2.);
    let world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        1,
        "MovementReferences",
        origin,
        0.,
    ));
    terrain.synchronize(Some(&world))?;
    let distance = resolve_world_view_distance(WorldViewDistanceRequest::new(
        777.,
        WorldMapId::new(571),
        0x8000_0000,
    ))?;
    let window = TerrainStreamingWindow::new(origin, distance, Vec3::new(777., 0., 0.))?;
    let cpu = CpuExecutor::new(CpuPoolConfig::new(
        NonZeroUsize::MIN,
        NonZeroUsize::new(2).ok_or("bad capacity")?,
    ))?;
    terrain.synchronize_streaming_async(571, origin, window, &cpu)?;
    cpu.try_submit(|| ())?.join()?;
    assert_eq!(
        terrain.synchronize_streaming_async(571, origin, window, &cpu)?,
        RuntimeTerrainStreamPoll::Current
    );
    let bounds = MovementCollisionBounds::new(Vec3::splat(-3.), Vec3::splat(3.))?;
    let mut query = RuntimeStaticMovementQuery::new();
    let expected = native_static_movement_owners()?;
    for phase in 0..3 {
        if phase == 1 {
            let world = ActiveWorld::enter(WorldBootstrap::new(
                WorldMapId::new(571),
                1,
                "MovementReferences",
                Vec3::new(1., -1., 2.),
                0.,
            ));
            assert_eq!(
                terrain.synchronize_async(Some(&world), &cpu)?,
                RuntimeTerrainPoll::TileLoaded {
                    map_id: 571,
                    tile: second
                }
            );
        }
        assert_eq!(
            terrain.collect_static_movement(
                571,
                bounds,
                MovementBspCacheMode::Enabled,
                &mut query
            )?,
            RuntimeStaticMovementResidency::Ready
        );
        let actual: Vec<_> = (0..query.triangles().len())
            .map(|index| query.owner(index))
            .collect();
        assert_eq!(
            actual,
            expected.iter().copied().map(Some).collect::<Vec<_>>()
        );
    }
    // Invalid boxes must not leave the previous successful candidate list usable.
    let outside = MovementCollisionBounds::new(Vec3::splat(40_000.), Vec3::splat(40_001.))?;
    assert!(
        terrain
            .collect_static_movement(571, outside, MovementBspCacheMode::Enabled, &mut query)
            .is_err()
    );
    assert_eq!(query.map_id(), None);
    assert!(query.triangles().is_empty());
    assert_eq!(query.owner(0), None);
    terrain.disconnect();
    assert_eq!(
        terrain.collect_static_movement(571, bounds, MovementBspCacheMode::Enabled, &mut query)?,
        RuntimeStaticMovementResidency::PendingMap
    );
    Ok(())
}

fn native_static_movement_owners() -> Result<Vec<RuntimeStaticMovementOwner>, Box<dyn Error>> {
    let fixture = include_str!("../fixtures/movement-residency-native.txt");
    fixture
        .lines()
        .find(|line| line.starts_with("ready 1 "))
        .ok_or("missing native resident query")?
        .split_whitespace()
        .skip(2)
        .map(|word| {
            let tag = word.parse::<u32>()?;
            Ok(match tag {
                7..=9 => RuntimeStaticMovementOwner::WorldModel { unique_id: tag },
                700 | 800 | 900 => RuntimeStaticMovementOwner::WorldModelDoodad {
                    world_model_unique_id: tag / 100,
                    doodad_index: 0,
                },
                10009 | 10010 => RuntimeStaticMovementOwner::TerrainDoodad {
                    unique_id: tag - 10000,
                },
                _ => return Err("unknown native diagnostic owner".into()),
            })
        })
        .collect()
}

fn movement_reference_adt(
    tile: TerrainTileIndex,
    extra_root: bool,
) -> Result<Vec<u8>, Box<dyn Error>> {
    const ORIGIN: f32 = 32.0 * 533.333_3;
    let mut builder = AdtBuilder::new()
        .with_version(AdtVersion::WotLK)
        .add_texture("tileset/fixture/grass.blp")
        .add_model("World/Fixture/Collision.m2")
        .add_wmo("World/Wmo/Fixture.wmo");
    for unique_id in [9, 10] {
        builder = builder.add_doodad_placement(DoodadPlacement {
            name_id: 0,
            unique_id,
            position: [ORIGIN, 0.5, ORIGIN],
            rotation: [0.; 3],
            scale: 1024,
            flags: 0,
        });
    }
    for unique_id in if extra_root {
        &[7, 8, 9][..]
    } else {
        &[7, 8][..]
    } {
        builder = builder.add_wmo_placement(WmoPlacement {
            name_id: 0,
            unique_id: *unique_id,
            position: [ORIGIN, 0., ORIGIN],
            rotation: [0.; 3],
            extents_min: [ORIGIN - 5., -1., ORIGIN - 5.],
            extents_max: [ORIGIN + 5., 3., ORIGIN + 5.],
            flags: 0,
            doodad_set: 0,
            name_set: 0,
            scale: 1024,
        });
    }
    let bytes = position_streaming_adt(tile, 10., builder.build()?.to_bytes()?)?;
    let wow_adt::ParsedAdt::Root(mut root) = wow_adt::parse_adt(&mut Cursor::new(bytes))? else {
        return Err("not a root ADT".into());
    };
    for chunk in &mut root.mcnk_chunks {
        chunk.header.n_doodad_refs = 3;
        chunk.header.n_map_obj_refs = if extra_root { 4 } else { 3 };
        let mut references = vec![1, 0, 1, 1, 0, 1];
        if extra_root {
            references.push(2);
        }
        chunk.refs = Some(wow_adt::chunks::mcnk::McrfChunk { references });
    }
    // The dependency reads MTXF through EOF; retain the fixture's one MTEX word.
    root.texture_flags = Some(wow_adt::chunks::MtxfChunk { flags: vec![0] });
    Ok(wow_adt::builder::BuiltAdt::from_root_adt(*root, None).to_bytes()?)
}

fn movement_reference_root_wmo() -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = root_wmo_fixture();
    let header = bytes
        .windows(4)
        .position(|tag| tag == b"DHOM")
        .ok_or("missing MOHD")?;
    set_u32(&mut bytes, header + 8 + 4, 2);
    let groups = bytes
        .windows(4)
        .position(|tag| tag == b"IGOM")
        .ok_or("missing MOGI")?;
    let group = bytes[groups + 8..groups + 40].to_vec();
    bytes.splice(groups + 40..groups + 40, group);
    set_u32(&mut bytes, groups + 4, 64);
    let doodads = bytes
        .windows(4)
        .position(|tag| tag == b"DDOM")
        .ok_or("missing MODD")?;
    set_f32(&mut bytes, doodads + 8 + 4, 1.);
    Ok(bytes)
}

fn movement_reference_group_wmo() -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = group_wmo_fixture();
    let group = bytes
        .windows(4)
        .position(|tag| tag == b"PGOM")
        .ok_or("missing MOGP")?;
    let size = read_u32(&bytes, group + 4)?;
    push_wmo_chunk(&mut bytes, *b"RDOM", &[0, 0, 0, 0]);
    set_u32(&mut bytes, group + 4, size + 12);
    Ok(bytes)
}

/// Authors repeated selected IDs and one deliberately unreferenced MDDF record.
fn streaming_placement_adt(
    tile: TerrainTileIndex,
    conflict: u8,
) -> Result<Vec<u8>, Box<dyn Error>> {
    const ORIGIN: f32 = 32.0 * 533.333_3;
    let doodad = DoodadPlacement {
        name_id: 0,
        unique_id: 9,
        position: [ORIGIN, if conflict == 1 { 1.5 } else { 0.5 }, ORIGIN],
        rotation: [0.; 3],
        scale: 1024,
        flags: 0,
    };
    let unselected = DoodadPlacement {
        position: [ORIGIN, 300., ORIGIN],
        ..doodad
    };
    let placement = WmoPlacement {
        name_id: 0,
        unique_id: 7,
        position: [ORIGIN, if conflict == 2 { 1. } else { 0. }, ORIGIN],
        rotation: [0.; 3],
        extents_min: [ORIGIN - 1., -1., ORIGIN - 1.],
        extents_max: [ORIGIN + 1., 1., ORIGIN + 1.],
        flags: 0,
        doodad_set: 0,
        name_set: 0,
        scale: 1024,
    };
    let bytes = AdtBuilder::new()
        .with_version(AdtVersion::WotLK)
        .add_texture("tileset/fixture/grass.blp")
        .add_model("World/Fixture/Collision.m2")
        .add_doodad_placement(doodad)
        .add_doodad_placement(unselected)
        .add_wmo("World/Wmo/Fixture.wmo")
        .add_wmo_placement(placement)
        .build()?
        .to_bytes()?;
    let bytes = add_last_chunk_object_references(bytes, &[0], &[0])?;
    position_streaming_adt(tile, 10., bytes)
}

/// Builds correctly addressed flat ADTs with different collision heights.
fn streaming_adt(tile: TerrainTileIndex, height: f32) -> Result<Vec<u8>, Box<dyn Error>> {
    let bytes = AdtBuilder::new()
        .with_version(AdtVersion::WotLK)
        .add_texture("tileset/fixture/grass.blp")
        .build()?
        .to_bytes()?;
    position_streaming_adt(tile, height, bytes)
}

/// Assigns terrain coordinates while retaining selected placement records.
fn position_streaming_adt(
    tile: TerrainTileIndex,
    height: f32,
    bytes: Vec<u8>,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let wow_adt::ParsedAdt::Root(mut root) = wow_adt::parse_adt(&mut Cursor::new(bytes))? else {
        return Err("not a root ADT".into());
    };
    root.texture_flags = Some(wow_adt::chunks::MtxfChunk { flags: vec![0] });
    for chunk in &mut root.mcnk_chunks {
        chunk.header.position = [
            17_066.666_f32 - (u32::from(tile.y()) * 16 + chunk.header.index_y) as f32 * 33.333_332,
            17_066.666_f32 - (u32::from(tile.x()) * 16 + chunk.header.index_x) as f32 * 33.333_332,
            height,
        ];
        chunk
            .heights
            .as_mut()
            .ok_or("missing heights")?
            .heights
            .fill(0.);
    }
    Ok(wow_adt::builder::BuiltAdt::from_root_adt(*root, None).to_bytes()?)
}

/// World entry admits only the exact player ADT and prepares each MCNK once.
#[test]
fn terrain_residency_follows_authoritative_player_tile() -> Result<(), Box<dyn Error>> {
    let map_table = map_table();
    // Terrain publication now admits its real material and surface together.
    let mut liquid_strings = b"\0tileset\\fixture\\grass.blp\0".to_vec();
    let depth_name = liquid_strings.len() as u32;
    liquid_strings.extend_from_slice(b"proceduralRiverDepthTex\0");
    let mut liquid = [0; 45];
    liquid[0] = 2;
    liquid[14] = 1;
    liquid[15] = 1;
    liquid[16] = depth_name;
    liquid[23] = 1.0_f32.to_bits();
    liquid[25] = 1.0_f32.to_bits();
    liquid[42] = 1250;
    let liquid_table = wdbc_fixture(&liquid, &liquid_strings);
    let wdt = terrain_wdt()?;
    let adt = append_stacked_liquid_fixture(
        AdtBuilder::new()
            .with_version(AdtVersion::WotLK)
            .add_texture("tileset/fixture/grass.blp")
            .build()?
            .to_bytes()?,
    );
    let fixture = ClientFixture::with_common_files(&[
        ("DBFilesClient\\LiquidType.dbc", &liquid_table),
        (
            "DBFilesClient\\LiquidMaterial.dbc",
            &wdbc_fixture(&[1, 0, 1], &[0]),
        ),
        ("DBFilesClient\\Map.dbc", &map_table),
        ("World\\Maps\\Northrend\\Northrend.wdt", &wdt),
        ("World\\Maps\\Northrend\\Northrend_21_30.adt", &adt),
        ("tileset\\fixture\\grass.blp", &bootstrap_texture_blp()),
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let catalog = ArchiveCatalog::discover(root, Locale::EnUs)?;
    let worker_catalog = catalog.clone();
    let mut store = AssetStore::mount(catalog)?;
    let maps = MapCatalog::load(&mut store)?;
    let animations = AnimationDataCatalog::load(&mut store)?;
    let creatures = CreatureCatalog::load(&mut store)?;
    let characters = CharacterAppearanceCatalog::load(&mut store)?;
    let races = CharacterRaceCatalog::load(&mut store)?;
    let helmet_visibility = HelmetGeosetVisibilityCatalog::load(&mut store)?;
    let start_outfits = CharacterStartOutfitCatalog::load(&mut store)?;
    let item_definitions = ItemDefinitionCatalog::load(&mut store)?;
    let item_displays = ItemDisplayCatalog::load(&mut store)?;
    let item_visuals = ItemVisualCatalog::load(&mut store)?;
    let particle_colors = ParticleColorCatalog::load(&mut store)?;
    let liquids = solarity_asset::LiquidTypeCatalog::load(&mut store)?;
    let assets = AssetStoreHandle::new(store);
    let mut player = RuntimePlayerPresentation::new(
        assets.clone(),
        RuntimePlayerCatalogs::new(
            animations,
            creatures,
            CreatureFamilyCatalog::default(),
            characters,
            races,
            helmet_visibility,
            start_outfits,
            RuntimePlayerItemCatalogs::new(item_definitions, item_displays, item_visuals),
            particle_colors,
        ),
    );
    let mut terrain =
        RuntimeTerrainCoordinator::new(assets, maps).with_worker_catalog(worker_catalog);
    let player_position = Vec3::new(1_000.0, 5_800.0, 250.0);
    let world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        0xF130_0000_0000_0001,
        "TerrainFixture",
        player_position,
        0.0,
    ));
    let tile = TerrainTileIndex::new(21, 30).ok_or("fixture tile is invalid")?;

    // World verification precedes the local player's create-object packet.
    // Presentation waits for those fields instead of inventing a body model.
    assert_eq!(
        player.synchronize(Some(&world))?,
        RuntimePlayerPoll::Pending
    );
    assert!(player.resident_model().is_none());
    assert!(player.camera_pose().is_none());
    assert!(player.world_camera(777.0).is_none());

    assert_eq!(
        terrain.synchronize(Some(&world))?,
        RuntimeTerrainPoll::TileLoaded { map_id: 571, tile }
    );
    assert_eq!(terrain.resident_tile().map(|tile| tile.index()), Some(tile));
    assert_eq!(
        terrain.resident_tile().map(|tile| tile.chunks().len()),
        Some(256)
    );
    let textures = terrain
        .resident_texture_sources()
        .ok_or("resident tile omitted its MTEX sources")?;
    assert_eq!(textures.len(), 1);
    assert_eq!(textures[0].path().as_str(), "TILESET\\FIXTURE\\GRASS.BLP");
    assert_eq!(textures[0].mip_dimensions(0), Some((2, 1)));
    let mesh = terrain
        .resident_mesh_plan()
        .ok_or("resident tile omitted its aggregate mesh plan")?;
    assert_eq!(mesh.tile(), tile);
    assert_eq!(mesh.textures(), [textures[0].path().clone()]);
    assert_eq!(mesh.chunks().len(), 256);
    let first_bounds = mesh.chunks()[0].bounds();
    let ray_x = (first_bounds[0][0] + first_bounds[1][0]) * 0.5;
    let ray_y = (first_bounds[0][1] + first_bounds[1][1]) * 0.5;
    let ray_start = Vec3::new(ray_x, ray_y, first_bounds[1][2] + 10.0);
    let ray_end = Vec3::new(ray_x, ray_y, first_bounds[0][2] - 10.0);
    let collision = terrain
        .trace_collision(ray_start, ray_end, 0.0, 1.0)?
        .ok_or("vertical ray missed fixture terrain")?;
    assert!(collision.fraction() > 0.0 && collision.fraction() < 1.0);
    assert!(collision.normal().z > 0.0);
    let traced_height = ray_start.lerp(ray_end, collision.fraction()).z;
    let sampled_height = terrain
        .controlled_player_terrain_height(ray_start)?
        .ok_or("point-height query missed fixture terrain")?;
    assert!((sampled_height - traced_height).abs() < 0.001);
    // Neither a shortened segment nor a cutoff just before the surface may
    // report that later surface. Camera corner probes retain this upper bound.
    assert!(
        terrain
            .trace_collision(ray_start, ray_end, 0.0, collision.fraction() - 0.000_05)?
            .is_none()
    );
    assert!(
        terrain
            .trace_collision(
                ray_start,
                ray_start.lerp(ray_end, collision.fraction() * 0.5),
                0.0,
                1.0
            )?
            .is_none()
    );
    assert!(
        terrain
            .trace_collision(ray_end, ray_start, 0.0, 1.0)?
            .is_none()
    );
    let liquid_x = (32.0 - 30.0 - 0.5 / 128.0) * 533.333_3;
    let liquid_y = (32.0 - 21.0 - 0.5 / 128.0) * 533.333_3;
    let highest = terrain
        .sample_liquid(liquid_x, liquid_y, None)?
        .ok_or("fixture liquid was not sampled")?;
    assert!((highest.height() - 200.0).abs() < 0.001);
    assert_eq!(highest.liquid_type(), 2);
    let submerged = terrain
        .camera_submerged_liquid(Vec3::new(liquid_x, liquid_y, 150.), &liquids)?
        .ok_or("camera liquid was not selected")?;
    assert_eq!(submerged.liquid_type, 2);
    assert_eq!(submerged.depth, 50.);
    let unit_liquid = terrain
        .unit_submerged_liquid(Vec3::new(liquid_x, liquid_y, 150.), &liquids)?
        .ok_or("unit liquid was not selected")?;
    assert_eq!(unit_liquid, submerged);
    assert!(
        terrain
            .camera_submerged_liquid(Vec3::new(liquid_x, liquid_y, 250.), &liquids)?
            .is_none()
    );
    assert!(
        terrain
            .camera_submerged_liquid(Vec3::new(liquid_x, liquid_y, -50.), &liquids)?
            .is_none()
    );
    assert!(highest.is_fishable());
    assert!(!highest.is_deep());
    assert_liquid_height(
        terrain.sample_liquid(liquid_x, liquid_y, Some(50.0))?,
        100.0,
    )?;
    assert_liquid_height(
        terrain.sample_liquid(liquid_x, liquid_y, Some(150.0))?,
        200.0,
    )?;
    assert_liquid_height(
        terrain.sample_liquid(liquid_x, liquid_y, Some(250.0))?,
        200.0,
    )?;
    assert!(
        terrain
            .sample_liquid(liquid_x + 100.0, liquid_y, None)?
            .is_none()
    );
    assert_eq!(
        terrain.synchronize(Some(&world))?,
        RuntimeTerrainPoll::Current { map_id: 571, tile }
    );

    // Visibility consumes an explicit camera; residency does not invent one.
    let frame = WorldCamera::stock(
        Vec3::new(100.0, -16.0, 2.0),
        Vec3::new(99.0, -16.0, 2.0),
        Vec3::Z,
        200.0,
    )
    .frame(1.0)?;
    let visible = terrain.visible_chunks(WorldFrustum::new(frame, WorldScreenWindow::FULL)?)?;
    assert!(!visible.is_empty());

    terrain.disconnect();
    let cpu = CpuExecutor::new(CpuPoolConfig::new(
        NonZeroUsize::MIN,
        NonZeroUsize::new(2).ok_or("invalid admission bound")?,
    ))?;
    assert!(terrain.prewarm_location(571, player_position.x, player_position.y, &cpu)?);
    assert!(!terrain.prewarm_location(571, player_position.x, player_position.y, &cpu)?);
    assert_eq!(
        terrain.synchronize_async(None, &cpu)?,
        RuntimeTerrainPoll::Idle
    );
    let first_poll = terrain.synchronize_async(Some(&world), &cpu)?;
    let prewarm_published = matches!(
        first_poll,
        RuntimeTerrainPoll::TileLoaded {
            map_id: 571,
            tile: loaded_tile,
        } if loaded_tile == tile
    );
    if !prewarm_published {
        assert_eq!(first_poll, RuntimeTerrainPoll::Pending { map_id: 571 });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match terrain.synchronize_async(Some(&world), &cpu)? {
                RuntimeTerrainPoll::Pending { .. } if std::time::Instant::now() < deadline => {
                    std::thread::yield_now();
                }
                RuntimeTerrainPoll::Pending { .. } => return Err("terrain worker timed out".into()),
                RuntimeTerrainPoll::TileLoaded {
                    map_id: 571,
                    tile: loaded_tile,
                } if loaded_tile == tile => break,
                poll => return Err(format!("unexpected terrain worker result: {poll:?}").into()),
            }
        }
    }
    assert_eq!(terrain.resident_tile().map(|tile| tile.index()), Some(tile));
    assert_eq!(
        terrain.synchronize_async(Some(&world), &cpu)?,
        RuntimeTerrainPoll::Current { map_id: 571, tile }
    );

    // Queue an old-world job behind a controlled worker barrier, retire its
    // world, and let it finish before requesting the identical map/tile. Stock
    // NEW_WORLD replaces ownership even when the destination ID is unchanged.
    terrain.disconnect();
    let started = std::sync::Arc::new(std::sync::Barrier::new(2));
    let worker_started = std::sync::Arc::clone(&started);
    let (release, wait) = std::sync::mpsc::channel();
    let blocker = cpu.try_submit(move || {
        worker_started.wait();
        wait.recv()
    })?;
    started.wait();
    assert!(terrain.prewarm_location(571, player_position.x, player_position.y, &cpu)?);
    terrain.disconnect();
    release.send(())?;
    blocker.join()??;
    // The sole worker's FIFO marker proves the retired terrain job has ended.
    cpu.try_submit(|| ())?.join()?;
    assert_eq!(
        terrain.synchronize_async(Some(&world), &cpu)?,
        RuntimeTerrainPoll::Pending { map_id: 571 },
    );
    assert!(terrain.resident_tile().is_none());

    assert_eq!(terrain.synchronize(None)?, RuntimeTerrainPoll::Idle);
    assert_eq!(player.synchronize(None)?, RuntimePlayerPoll::Idle);
    assert!(terrain.active_map().is_none());
    assert!(terrain.resident_texture_sources().is_none());
    assert!(terrain.resident_mesh_plan().is_none());
    Ok(())
}

/// Visible creature create, movement, and out-of-range state drives M2 residency.
#[test]
fn creature_residency_tracks_authoritative_world_lifecycle() -> Result<(), Box<dyn Error>> {
    let display = creature_display_table();
    let model_data = creature_model_table();
    let m2 = m2_collision_fixture()?;
    let skin = skin_fixture()?;
    let texture = bootstrap_texture_blp();
    let fixture = ClientFixture::with_common_files(&[
        ("DBFilesClient\\CreatureDisplayInfo.dbc", &display),
        ("DBFilesClient\\CreatureModelData.dbc", &model_data),
        ("World\\Fixture\\Collision.m2", &m2),
        ("World\\Fixture\\Collision00.skin", &skin),
        ("World\\Fixture\\Collision.blp", &texture),
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let animations = AnimationDataCatalog::load(&mut store)?;
    let creatures = CreatureCatalog::load(&mut store)?;
    let characters = CharacterAppearanceCatalog::load(&mut store)?;
    let races = CharacterRaceCatalog::load(&mut store)?;
    let helmet_visibility = HelmetGeosetVisibilityCatalog::load(&mut store)?;
    let start_outfits = CharacterStartOutfitCatalog::load(&mut store)?;
    let item_definitions = ItemDefinitionCatalog::load(&mut store)?;
    let item_displays = ItemDisplayCatalog::load(&mut store)?;
    let item_visuals = ItemVisualCatalog::load(&mut store)?;
    let particle_colors = ParticleColorCatalog::load(&mut store)?;
    let assets = AssetStoreHandle::new(store);
    let mut presentation = RuntimePlayerPresentation::new(
        assets,
        RuntimePlayerCatalogs::new(
            animations,
            creatures,
            CreatureFamilyCatalog::default(),
            characters,
            races,
            helmet_visibility,
            start_outfits,
            RuntimePlayerItemCatalogs::new(item_definitions, item_displays, item_visuals),
            particle_colors,
        ),
    );
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        30,
        "Local",
        Vec3::ZERO,
        0.0,
    ));
    let guid = 20;
    let fields = [
        (4, 1.0_f32.to_bits()),
        (67, 100),
        (68, 100),
        (69, 0),
        (74, u32::from_le_bytes([0, 0, 0, 0])),
        (122, 0),
    ];
    world.create_object(
        guid,
        solarity_ecs::ObjectKind::Unit,
        Some(WorldTransform::new(Vec3::X, 0.5)),
        fields,
    )?;
    project_object_fields(&mut world, guid, fields)?;

    assert_eq!(
        presentation.synchronize_creatures(Some(&world), |_| None)?,
        RuntimeCreaturePoll::ModelsChanged
    );
    assert_eq!(presentation.resident_creature_count(), 1);
    world.update_transform(guid, WorldTransform::new(Vec3::Y, 1.0))?;
    assert_eq!(
        presentation.synchronize_creatures(Some(&world), |_| None)?,
        RuntimeCreaturePoll::Current
    );
    world.remove_object(guid)?;
    assert_eq!(
        presentation.synchronize_creatures(Some(&world), |_| None)?,
        RuntimeCreaturePoll::ModelsChanged
    );
    assert_eq!(presentation.resident_creature_count(), 0);
    Ok(())
}

/// Visible players use the complete character composer and leave with range state.
#[test]
fn remote_player_residency_tracks_authoritative_world_lifecycle() -> Result<(), Box<dyn Error>> {
    let display = mounted_player_display_table();
    let model_data = mounted_player_model_table();
    let character = character_section_tables();
    let races = character_race_table();
    let m2 = m2_collision_fixture()?;
    let skin = skin_fixture()?;
    let body_texture = solid_raw3_blp(256, 256, 0xFFFF_FFFF);
    let fixture = ClientFixture::with_common_files(&[
        ("DBFilesClient\\CreatureDisplayInfo.dbc", &display),
        ("DBFilesClient\\CreatureModelData.dbc", &model_data),
        ("DBFilesClient\\CharSections.dbc", &character.sections),
        (
            "DBFilesClient\\CharHairGeosets.dbc",
            &character.hair_geosets,
        ),
        (
            "DBFilesClient\\CharacterFacialHairStyles.dbc",
            &character.facial_hair,
        ),
        ("DBFilesClient\\ChrRaces.dbc", &races),
        ("Character\\Human\\Male\\HumanMale.m2", &m2),
        ("Character\\Human\\Male\\HumanMale00.skin", &skin),
        ("Character\\Human\\Male\\Skin.blp", &body_texture),
        ("World\\Fixture\\Collision.blp", &bootstrap_texture_blp()),
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let animations = AnimationDataCatalog::load(&mut store)?;
    let creatures = CreatureCatalog::load(&mut store)?;
    let characters = CharacterAppearanceCatalog::load(&mut store)?;
    let races = CharacterRaceCatalog::load(&mut store)?;
    let helmet_visibility = HelmetGeosetVisibilityCatalog::load(&mut store)?;
    let start_outfits = CharacterStartOutfitCatalog::load(&mut store)?;
    let item_definitions = ItemDefinitionCatalog::load(&mut store)?;
    let item_displays = ItemDisplayCatalog::load(&mut store)?;
    let item_visuals = ItemVisualCatalog::load(&mut store)?;
    let particle_colors = ParticleColorCatalog::load(&mut store)?;
    let assets = AssetStoreHandle::new(store);
    let mut presentation = RuntimePlayerPresentation::new(
        assets,
        RuntimePlayerCatalogs::new(
            animations,
            creatures,
            CreatureFamilyCatalog::default(),
            characters,
            races,
            helmet_visibility,
            start_outfits,
            RuntimePlayerItemCatalogs::new(item_definitions, item_displays, item_visuals),
            particle_colors,
        ),
    );
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        30,
        "Local",
        Vec3::ZERO,
        0.0,
    ));
    let guid = 20;
    let fields = [
        (4, 1.0_f32.to_bits()),
        (23, u32::from_le_bytes([1, 1, 0, 0])),
        (67, 100),
        (68, 100),
        (69, 200),
        (74, 0),
        (122, 0),
        (153, 0),
        (154, 0),
    ];
    world.create_object(
        guid,
        solarity_ecs::ObjectKind::Player,
        Some(WorldTransform::new(Vec3::X, 0.5)),
        fields,
    )?;
    project_object_fields(&mut world, guid, fields)?;

    assert_eq!(
        presentation.synchronize_remote_players(Some(&world))?,
        RuntimeRemotePlayerPoll::ModelsChanged
    );
    assert_eq!(presentation.resident_remote_player_count(), 1);
    assert_eq!(presentation.resident_remote_mount_count(), 1);
    let level_eight = CharacterComponentTextureLevel::new(8)
        .ok_or("stock component texture level eight was rejected")?;
    assert!(presentation.set_component_texture_level(level_eight));
    assert_eq!(presentation.resident_remote_player_count(), 0);
    assert_eq!(
        presentation.synchronize_remote_players(Some(&world))?,
        RuntimeRemotePlayerPoll::ModelsChanged
    );
    assert_eq!(presentation.resident_remote_player_count(), 1);
    assert!(!presentation.set_component_texture_level(level_eight));
    world.update_transform(guid, WorldTransform::new(Vec3::Y, 1.0))?;
    assert_eq!(
        presentation.synchronize_remote_players(Some(&world))?,
        RuntimeRemotePlayerPoll::Current
    );
    let dismount = [(69, 0)];
    world.update_fields(guid, dismount)?;
    project_object_fields(&mut world, guid, dismount)?;
    assert_eq!(
        presentation.synchronize_remote_players(Some(&world))?,
        RuntimeRemotePlayerPoll::ModelsChanged
    );
    assert_eq!(presentation.resident_remote_mount_count(), 0);
    world.remove_object(guid)?;
    assert_eq!(
        presentation.synchronize_remote_players(Some(&world))?,
        RuntimeRemotePlayerPoll::ModelsChanged
    );
    assert_eq!(presentation.resident_remote_player_count(), 0);
    assert_eq!(
        presentation.synchronize_remote_players(None)?,
        RuntimeRemotePlayerPoll::Idle
    );
    Ok(())
}

/// MCNK references admit one shared MODF generation into camera collision.
#[test]
fn terrain_residency_admits_referenced_world_models() -> Result<(), Box<dyn Error>> {
    const CLIENT_MAP_ORIGIN: f32 = 32.0 * 533.333_3;
    let placement = WmoPlacement {
        name_id: 0,
        unique_id: 7,
        position: [CLIENT_MAP_ORIGIN, 0.0, CLIENT_MAP_ORIGIN],
        rotation: [0.0, 0.0, 0.0],
        extents_min: [CLIENT_MAP_ORIGIN - 1.0, -1.0, CLIENT_MAP_ORIGIN - 1.0],
        extents_max: [CLIENT_MAP_ORIGIN + 1.0, 1.0, CLIENT_MAP_ORIGIN + 1.0],
        flags: 0,
        doodad_set: 0,
        name_set: 0,
        scale: 1024,
    };
    let doodad = DoodadPlacement {
        name_id: 0,
        unique_id: 9,
        position: [CLIENT_MAP_ORIGIN, 0.5, CLIENT_MAP_ORIGIN],
        rotation: [0.0, 0.0, 0.0],
        scale: 1_024,
        flags: 0,
    };
    let adt = add_last_chunk_object_references(
        AdtBuilder::new()
            .with_version(AdtVersion::WotLK)
            .add_texture("tileset/fixture/grass.blp")
            .add_model("World/Fixture/Collision.m2")
            .add_doodad_placement(doodad)
            .add_wmo("World/Wmo/Fixture.wmo")
            .add_wmo_placement(placement)
            .build()?
            .to_bytes()?,
        &[0],
        &[0],
    )?;
    let root_wmo = root_wmo_fixture();
    let group_wmo = movement_reference_group_wmo()?;
    let m2 = m2_collision_fixture()?;
    let skin = skin_fixture()?;
    let fixture = world_model_client_fixture(&[
        ("DBFilesClient\\Map.dbc", &map_table()),
        ("World\\Maps\\Northrend\\Northrend.wdt", &terrain_wdt()?),
        ("World\\Maps\\Northrend\\Northrend_21_30.adt", &adt),
        ("tileset\\fixture\\grass.blp", &bootstrap_texture_blp()),
        ("World\\Wmo\\Fixture.wmo", &root_wmo),
        ("World\\Wmo\\Fixture_000.wmo", &group_wmo),
        ("World\\Fixture\\Collision.m2", &m2),
        ("World\\Fixture\\Collision00.skin", &skin),
        ("World\\Fixture\\Collision.blp", &bootstrap_texture_blp()),
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let maps = MapCatalog::load(&mut store)?;
    let assets = AssetStoreHandle::new(store);
    let mut terrain = RuntimeTerrainCoordinator::new(assets, maps);
    let world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        0xF130_0000_0000_0001,
        "WorldModelFixture",
        Vec3::new(1_000.0, 5_800.0, 250.0),
        0.0,
    ));

    terrain.synchronize(Some(&world))?;
    assert_eq!(terrain.resident_world_model_count(), 1);
    assert_eq!(terrain.resident_world_model_source_count(), 1);
    assert_eq!(terrain.resident_m2_count(), 2);
    assert_eq!(terrain.resident_m2_source_count(), 1);
    assert_eq!(terrain.resident_m2_authored_texture_count(), 1);
    assert_eq!(terrain.resident_m2_replaceable_texture_count(), 1);
    assert_eq!(terrain.resident_m2_collision_count(), 2);
    let m2_hit = terrain
        .trace_m2_camera(
            Vec3::new(-0.25, -0.25, 1.0),
            Vec3::new(-0.25, -0.25, -1.0),
            1.0,
        )?
        .ok_or("camera ray missed resident M2")?;
    assert!((m2_hit - 0.25).abs() < 0.001);
    let hit = terrain
        .trace_world_model_camera(
            Vec3::new(-0.25, -0.25, 1.0),
            Vec3::new(-0.25, -0.25, -1.0),
            1.0,
        )?
        .ok_or("camera ray missed resident WMO")?;
    assert!((hit - 0.5).abs() < 0.001);
    let liquid = terrain
        .sample_world_model_liquid(-1.0, -1.0, Some(1.0))?
        .ok_or("resident WMO liquid was not sampled")?;
    assert!((liquid.height() - 2.0).abs() < 0.001);
    assert_eq!(liquid.liquid_type(), 14);
    assert!(liquid.is_fishable());
    let height = resolve_camera_subject_height(CameraSubjectGeometry::new(Some(1.75), 2.0, 1.0))?;
    let pose = resolve_player_camera_pose(
        WorldTransform::new(Vec3::ZERO, 0.0),
        PlayerViewState::new(2.0, 0.174_532_92, 0.0, 2),
        height,
    )?;
    let resolved = terrain.resolve_player_camera(
        pose,
        1.0,
        solarity_systems::PlayerCameraObstructionSettings::default(),
    )?;
    // Native 6059E0 over this MLIQ quad, followed by the one-ninth retreat,
    // gives distance 0.26489076 and eye Z 1.8932198. This synthetic group
    // omits MOGP 0x1000: volumes admit the mesh, while water rays skip it.
    assert!(
        resolved
            .eye()
            .abs_diff_eq(Vec3::new(-0.260_866_46, 0.0, 1.893_219_8), 0.000_02),
        "{resolved:?}"
    );

    terrain.disconnect();
    assert_eq!(terrain.resident_world_model_count(), 0);
    assert_eq!(terrain.resident_world_model_source_count(), 0);
    assert_eq!(terrain.resident_m2_count(), 0);
    assert_eq!(terrain.resident_m2_source_count(), 0);
    assert_eq!(terrain.resident_m2_authored_texture_count(), 0);
    assert_eq!(terrain.resident_m2_replaceable_texture_count(), 0);
    assert_eq!(terrain.resident_m2_collision_count(), 0);
    Ok(())
}

/// WDT-level MODF state reaches rendering, queries, and area publication.
#[test]
fn global_world_model_residency_completes_the_scene() -> Result<(), Box<dyn Error>> {
    let root_wmo = root_wmo_fixture();
    let group_wmo = movement_reference_group_wmo()?;
    let m2 = m2_collision_fixture()?;
    let skin = skin_fixture()?;
    let fixture = world_model_client_fixture(&[
        ("DBFilesClient\\Map.dbc", &map_table()),
        ("World\\Maps\\Northrend\\Northrend.wdt", &global_wmo_wdt()?),
        ("World\\Wmo\\Fixture.wmo", &root_wmo),
        ("World\\Wmo\\Fixture_000.wmo", &group_wmo),
        ("World\\Fixture\\Collision.m2", &m2),
        ("World\\Fixture\\Collision00.skin", &skin),
        ("World\\Fixture\\Collision.blp", &bootstrap_texture_blp()),
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let maps = MapCatalog::load(&mut store)?;
    let registration_model = std::sync::Arc::new(solarity_asset::DecodedM2Model::load(
        &mut store,
        &solarity_asset::AssetPath::new("World\\Fixture\\Collision.m2")?,
    )?);
    let assets = AssetStoreHandle::new(store);
    let mut terrain = RuntimeTerrainCoordinator::new(assets, maps);
    let world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        0xF130_0000_0000_0001,
        "GlobalWorldModelFixture",
        Vec3::ZERO,
        0.0,
    ));

    assert_eq!(
        terrain.synchronize(Some(&world))?,
        RuntimeTerrainPoll::GlobalWorldModelLoaded { map_id: 571 }
    );
    let mut registered_model = PlacedM2Collision::prepare_transform(
        registration_model,
        glam::Mat4::from_translation(Vec3::new(-1.25, -1.25, 0.)),
    )?;
    let mut registration = RuntimeMovementRegistrationQuery::new();
    assert_eq!(
        terrain.register_game_object_movement(
            571,
            &registered_model,
            MovementBspCacheMode::Enabled,
            &mut registration
        )?,
        RuntimeStaticMovementResidency::Ready
    );
    assert_eq!(
        registration.references(),
        &[RuntimeMovementReference::WorldModel {
            unique_id: 7,
            group: 0
        }]
    );
    let selected = registration
        .selection()
        .and_then(|result| result.selected())
        .ok_or("global WMO registration lost the interior floor")?;
    assert_eq!(
        selected.owner(),
        solarity_runtime::RuntimeWorldModelMovementOwner::Static { unique_id: 7 }
    );
    assert!(selected.hit().is_interior());
    assert_eq!(selected.hit().face(), Some(0));
    registered_model.set_transform(glam::Mat4::from_translation(Vec3::new(
        40_000., 40_000., 0.,
    )))?;
    assert_eq!(
        terrain.register_game_object_movement(
            571,
            &registered_model,
            MovementBspCacheMode::Enabled,
            &mut registration
        )?,
        RuntimeStaticMovementResidency::Ready
    );
    assert!(registration.references().is_empty());
    assert!(
        registration
            .selection()
            .and_then(|result| result.selected())
            .is_none()
    );
    let mut movement = RuntimeStaticMovementQuery::new();
    let bounds = MovementCollisionBounds::new(Vec3::splat(-1.), Vec3::splat(1.))?;
    assert_eq!(
        terrain.collect_static_movement(
            571,
            bounds,
            MovementBspCacheMode::Enabled,
            &mut movement
        )?,
        RuntimeStaticMovementResidency::Ready
    );
    assert_eq!(movement.triangles().len(), 1);
    assert_eq!(
        movement.owner(0),
        Some(RuntimeStaticMovementOwner::WorldModel { unique_id: 7 })
    );
    let outside_grid = MovementCollisionBounds::new(Vec3::splat(40_000.), Vec3::splat(40_001.))?;
    assert_eq!(
        terrain.collect_static_movement(
            571,
            outside_grid,
            MovementBspCacheMode::Enabled,
            &mut movement
        )?,
        RuntimeStaticMovementResidency::Ready
    );
    assert!(movement.triangles().is_empty());
    assert!(terrain.resident_tile().is_none());
    assert!(terrain.resident_mesh_plan().is_none());
    assert_eq!(terrain.current_area_id(&world)?, Some(571));
    assert_eq!(terrain.resident_world_model_count(), 1);
    assert_eq!(terrain.resident_world_model_source_count(), 1);
    assert_eq!(terrain.resident_m2_count(), 1);
    assert_eq!(terrain.resident_m2_source_count(), 1);
    assert_eq!(terrain.resident_m2_collision_count(), 1);
    assert!(
        terrain
            .trace_world_model_camera(
                Vec3::new(-0.25, -0.25, 1.0),
                Vec3::new(-0.25, -0.25, -1.0),
                1.0,
            )?
            .is_some()
    );
    assert!(
        terrain
            .sample_world_model_liquid(-1.0, -1.0, Some(1.0))?
            .is_some()
    );
    assert_eq!(
        terrain.synchronize(Some(&world))?,
        RuntimeTerrainPoll::GlobalWorldModelCurrent { map_id: 571 }
    );
    assert_eq!(terrain.synchronize(None)?, RuntimeTerrainPoll::Idle);
    assert_eq!(terrain.resident_world_model_count(), 0);
    Ok(())
}

fn terrain_wdt() -> Result<Vec<u8>, Box<dyn Error>> {
    let mut wdt = WdtFile::new(WowVersion::WotLK);
    wdt.mwmo = Some(MwmoChunk::new());
    let entry = wdt
        .main
        .get_mut(21, 30)
        .ok_or("fixture WDT tile is invalid")?;
    entry.set_has_adt(true);
    let mut bytes = Vec::new();
    WdtWriter::new(&mut bytes).write(&wdt)?;
    Ok(bytes)
}

fn global_wmo_wdt() -> Result<Vec<u8>, Box<dyn Error>> {
    let mut wdt = WdtFile::new(WowVersion::WotLK);
    wdt.mphd.flags |= MphdFlags::WDT_USES_GLOBAL_MAP_OBJ;
    wdt.mwmo = Some(MwmoChunk {
        filenames: vec!["World\\Wmo\\Fixture.wmo".to_owned()],
    });
    wdt.modf = Some(ModfChunk {
        entries: vec![ModfEntry {
            id: 0,
            unique_id: 7,
            position: [0.0; 3],
            rotation: [0.0; 3],
            lower_bounds: [-5.0, -1.0, -5.0],
            upper_bounds: [5.0, 3.0, 5.0],
            flags: 0,
            doodad_set: 0,
            name_set: 0,
            scale: 0,
        }],
    });
    let mut bytes = Vec::new();
    WdtWriter::new(&mut bytes).write(&wdt)?;
    Ok(bytes)
}

fn assert_liquid_height(
    sample: Option<solarity_systems::TerrainLiquidSample>,
    expected: f32,
) -> Result<(), Box<dyn Error>> {
    let sample = sample.ok_or("fixture liquid was not sampled")?;
    assert!((sample.height() - expected).abs() < 0.001);
    Ok(())
}

fn map_table() -> Vec<u8> {
    let mut strings = vec![0_u8];
    let directory = append_string(&mut strings, "Northrend");
    let name = append_string(&mut strings, "Northrend");
    let mut fields = [0_u32; 66];
    fields[0] = 571;
    fields[1] = directory;
    fields[5] = name;
    fields[22] = 571;
    fields[59] = u32::MAX;
    fields[63] = 2;
    let mut bytes = Vec::with_capacity(20 + fields.len() * 4 + strings.len());
    bytes.extend_from_slice(b"WDBC");
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&66_u32.to_le_bytes());
    bytes.extend_from_slice(&(66_u32 * 4).to_le_bytes());
    bytes.extend_from_slice(&(strings.len() as u32).to_le_bytes());
    for field in fields {
        bytes.extend_from_slice(&field.to_le_bytes());
    }
    bytes.extend_from_slice(&strings);
    bytes
}

fn creature_display_table() -> Vec<u8> {
    let mut fields = [0_u32; 16];
    fields[0] = 100;
    fields[1] = 7;
    fields[4] = 1.0_f32.to_bits();
    fields[5] = u32::MAX;
    wdbc_fixture(&fields, b"\0")
}

fn creature_model_table() -> Vec<u8> {
    let mut strings = vec![0_u8];
    let path = append_string(&mut strings, "World\\Fixture\\Collision.m2");
    let mut fields = [0_u32; 28];
    fields[0] = 7;
    fields[2] = path;
    fields[4] = 1.0_f32.to_bits();
    wdbc_fixture(&fields, &strings)
}

fn mounted_player_display_table() -> Vec<u8> {
    let mut fields = vec![0_u32; 32];
    fields[0] = 100;
    fields[1] = 7;
    fields[4] = 1.0_f32.to_bits();
    fields[5] = u32::MAX;
    fields[16] = 200;
    fields[17] = 8;
    fields[20] = 1.5_f32.to_bits();
    fields[21] = u32::MAX;
    create_wdbc_fixture(2, 16, &fields, b"\0")
}

fn mounted_player_model_table() -> Vec<u8> {
    let mut strings = vec![0_u8];
    let path = append_string(&mut strings, "Character\\Human\\Male\\HumanMale.m2");
    let mut fields = vec![0_u32; 56];
    fields[0] = 7;
    fields[2] = path;
    fields[4] = 1.0_f32.to_bits();
    fields[28] = 8;
    fields[30] = path;
    fields[32] = 2.0_f32.to_bits();
    fields[44] = 3.25_f32.to_bits();
    create_wdbc_fixture(2, 28, &fields, &strings)
}

struct CharacterSectionTables {
    sections: Vec<u8>,
    hair_geosets: Vec<u8>,
    facial_hair: Vec<u8>,
}

fn character_section_tables() -> CharacterSectionTables {
    let mut strings = vec![0_u8];
    let skin = append_string(&mut strings, "Character\\Human\\Male\\Skin.blp");
    let mut fields = [
        10, 1, 0, 0, skin, 0, 0, 8, 0, 0, 11, 1, 0, 1, 0, 0, 0, 0, 0, 0, 12, 1, 0, 2, 0, 0, 0, 0,
        0, 0, 13, 1, 0, 3, 0, 0, 0, 0, 0, 0, 14, 1, 0, 4, 0, 0, 0, 0, 0, 0,
    ];
    // Remote players use the class-aware component path. Every synthetic
    // section must carry stock's player-eligible flag, including this fixture's
    // NPC-style skin, which deliberately omits face and underwear overlays.
    for section in fields.as_chunks_mut::<10>().0 {
        section[7] |= 0x01;
    }
    CharacterSectionTables {
        sections: create_wdbc_fixture(5, 10, &fields, &strings),
        hair_geosets: create_wdbc_fixture(0, 6, &[], b"\0"),
        facial_hair: create_wdbc_fixture(0, 8, &[], b"\0"),
    }
}

fn character_race_table() -> Vec<u8> {
    let mut strings = vec![0_u8];
    let prefix = append_string(&mut strings, "Hu");
    let file_string = append_string(&mut strings, "Human");
    let name = append_string(&mut strings, "Human");
    let mut fields = [0_u32; 69];
    fields[0] = 1;
    fields[4] = 100;
    fields[5] = 101;
    fields[6] = prefix;
    fields[11] = file_string;
    fields[14] = name;
    wdbc_fixture(&fields, &strings)
}

fn wdbc_fixture(fields: &[u32], strings: &[u8]) -> Vec<u8> {
    create_wdbc_fixture(1, fields.len() as u32, fields, strings)
}

fn create_wdbc_fixture(
    record_count: u32,
    field_count: u32,
    fields: &[u32],
    strings: &[u8],
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(20 + fields.len() * 4 + strings.len());
    bytes.extend_from_slice(b"WDBC");
    bytes.extend_from_slice(&record_count.to_le_bytes());
    bytes.extend_from_slice(&field_count.to_le_bytes());
    bytes.extend_from_slice(&(field_count * 4).to_le_bytes());
    bytes.extend_from_slice(&(strings.len() as u32).to_le_bytes());
    for field in fields {
        bytes.extend_from_slice(&field.to_le_bytes());
    }
    bytes.extend_from_slice(strings);
    bytes
}

fn solid_raw3_blp(width: u32, height: u32, color: u32) -> Vec<u8> {
    const HEADER_SIZE: u32 = 148;
    const PALETTE_SIZE: u32 = 256 * 4;
    const PIXEL_OFFSET: u32 = HEADER_SIZE + PALETTE_SIZE;
    let pixel_count = width.saturating_mul(height);
    let pixel_bytes = pixel_count.saturating_mul(4);

    let mut bytes = Vec::with_capacity((PIXEL_OFFSET + pixel_bytes) as usize);
    bytes.extend_from_slice(b"BLP2");
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&[3, 8, 8, 0]);
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes.extend_from_slice(&PIXEL_OFFSET.to_le_bytes());
    for _unused in 1..16 {
        bytes.extend_from_slice(&0_u32.to_le_bytes());
    }
    bytes.extend_from_slice(&pixel_bytes.to_le_bytes());
    for _unused in 1..16 {
        bytes.extend_from_slice(&0_u32.to_le_bytes());
    }
    bytes.resize(PIXEL_OFFSET as usize, 0);
    for _pixel in 0..pixel_count {
        bytes.extend_from_slice(&color.to_le_bytes());
    }
    bytes
}

fn append_string(block: &mut Vec<u8>, value: &str) -> u32 {
    let offset = block.len() as u32;
    block.extend_from_slice(value.as_bytes());
    block.push(0);
    offset
}

/// Appends two overlapping planar MH2O layers for stacked-surface selection.
fn append_stacked_liquid_fixture(mut adt: Vec<u8>) -> Vec<u8> {
    const INSTANCE_OFFSET: usize = 256 * 12;
    const ATTRIBUTES_OFFSET: usize = INSTANCE_OFFSET + 2 * 24;
    const LOWER_VERTEX_OFFSET: usize = ATTRIBUTES_OFFSET + 16;
    const UPPER_VERTEX_OFFSET: usize = LOWER_VERTEX_OFFSET + 20;
    let mut payload = vec![0_u8; UPPER_VERTEX_OFFSET + 20];
    set_u32(&mut payload, 0, INSTANCE_OFFSET as u32);
    set_u32(&mut payload, 4, 2);
    set_u32(&mut payload, 8, ATTRIBUTES_OFFSET as u32);
    write_liquid_instance(&mut payload, INSTANCE_OFFSET, 100.0, LOWER_VERTEX_OFFSET);
    write_liquid_instance(
        &mut payload,
        INSTANCE_OFFSET + 24,
        200.0,
        UPPER_VERTEX_OFFSET,
    );
    set_u64(&mut payload, ATTRIBUTES_OFFSET, 1);
    write_height_depth_vertices(&mut payload, LOWER_VERTEX_OFFSET, 100.0);
    write_height_depth_vertices(&mut payload, UPPER_VERTEX_OFFSET, 200.0);
    adt.extend_from_slice(b"O2HM");
    adt.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    adt.extend_from_slice(&payload);
    adt
}

fn write_liquid_instance(bytes: &mut [u8], offset: usize, height: f32, vertices: usize) {
    set_u16(bytes, offset, 2);
    set_u16(bytes, offset + 2, 0);
    set_f32(bytes, offset + 4, height);
    set_f32(bytes, offset + 8, height);
    bytes[offset + 14] = 1;
    bytes[offset + 15] = 1;
    set_u32(bytes, offset + 20, vertices as u32);
}

fn write_height_depth_vertices(bytes: &mut [u8], offset: usize, height: f32) {
    for index in 0..4 {
        set_f32(bytes, offset + index * 4, height);
    }
    bytes[offset + 16..offset + 20].fill(u8::MAX);
}

fn set_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn set_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn set_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn set_f32(bytes: &mut [u8], offset: usize, value: f32) {
    set_u32(bytes, offset, value.to_bits());
}

/// Adds MCRF MDDF and MODF references to the final generated MCNK without
/// moving any later indexed terrain chunk. The fixture builder omits MCRF APIs.
fn add_last_chunk_object_references(
    mut adt: Vec<u8>,
    doodads: &[u32],
    world_models: &[u32],
) -> Result<Vec<u8>, Box<dyn Error>> {
    let chunk_start = adt
        .windows(4)
        .rposition(|window| window == b"KNCM")
        .ok_or("fixture ADT omits MCNK")?;
    let old_size = read_u32(&adt, chunk_start + 4)? as usize;
    let chunk_end = chunk_start + 8 + old_size;
    set_u32(&mut adt, chunk_start + 8 + 0x20, (8 + old_size) as u32);
    set_u32(
        &mut adt,
        chunk_start + 8 + 0x10,
        u32::try_from(doodads.len())?,
    );
    set_u32(
        &mut adt,
        chunk_start + 8 + 0x38,
        u32::try_from(world_models.len())?,
    );
    let payload_size = (doodads.len() + world_models.len()) * 4;
    let mut reference = Vec::with_capacity(8 + payload_size);
    reference.extend_from_slice(b"FRCM");
    reference.extend_from_slice(&u32::try_from(payload_size)?.to_le_bytes());
    for index in doodads.iter().chain(world_models) {
        reference.extend_from_slice(&index.to_le_bytes());
    }
    adt.splice(chunk_end..chunk_end, reference);
    let added_size = 8 + payload_size;
    set_u32(
        &mut adt,
        chunk_start + 4,
        u32::try_from(old_size + added_size)?,
    );

    let mcin_start = adt
        .windows(4)
        .position(|window| window == b"NICM")
        .ok_or("fixture ADT omits MCIN")?;
    set_u32(
        &mut adt,
        mcin_start + 8 + 255 * 16 + 4,
        u32::try_from(old_size + added_size)?,
    );
    Ok(adt)
}

fn m2_collision_fixture() -> Result<Vec<u8>, Box<dyn Error>> {
    let texture_name = b"World\\Fixture\\Collision.blp";
    let mut model = M2Model {
        header: M2Header::new(M2Version::WotLK),
        name: Some("Collision".to_owned()),
        ..M2Model::default()
    };
    model.header.num_skin_profiles = Some(1);
    model.header.bounding_box_min = [-1.0; 3];
    model.header.bounding_box_max = [2.0; 3];
    model.header.bounding_sphere_radius = 3.0;
    model.header.collision_box_min = [0.0, 0.0, -0.1];
    model.header.collision_box_max = [2.0, 2.0, 0.1];
    model.header.collision_sphere_radius = 2.0_f32.sqrt();
    model.textures = vec![
        RawTexture {
            texture_type: M2TextureType::Hardcoded,
            flags: M2TextureFlags::empty(),
            filename: M2ArrayString {
                string: FixedString {
                    data: texture_name.to_vec(),
                },
                array: M2Array::new(u32::try_from(texture_name.len() + 1)?, 1),
            },
        },
        RawTexture {
            texture_type: M2TextureType::Monster1,
            flags: M2TextureFlags::empty(),
            filename: M2ArrayString::default(),
        },
    ];
    for index in [0_u16, 1, 2] {
        model
            .raw_data
            .bounding_triangles
            .extend_from_slice(&index.to_le_bytes());
    }
    for position in [[0.0_f32, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]] {
        for component in position {
            model
                .raw_data
                .bounding_vertices
                .extend_from_slice(&component.to_le_bytes());
        }
        model.vertices.push(M2Vertex {
            position: C3Vector {
                x: position[0],
                y: position[1],
                z: position[2],
            },
            bone_weights: [0; 4],
            bone_indices: [0; 4],
            normal: C3Vector {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
            tex_coords: C2Vector { x: 0.0, y: 0.0 },
            tex_coords2: Some(C2Vector { x: 0.0, y: 0.0 }),
        });
    }
    for component in [0.0_f32, 0.0, 1.0] {
        model
            .raw_data
            .bounding_normals
            .extend_from_slice(&component.to_le_bytes());
    }
    let mut cursor = Cursor::new(Vec::new());
    model.write(&mut cursor)?;
    let mut bytes = cursor.into_inner();
    let texture_array_offset = u32::from_le_bytes(bytes[0x54..0x58].try_into()?) as usize;
    let filename_offset = u32::try_from(bytes.len())?;
    bytes[texture_array_offset + 8..texture_array_offset + 12]
        .copy_from_slice(&u32::try_from(texture_name.len() + 1)?.to_le_bytes());
    bytes[texture_array_offset + 12..texture_array_offset + 16]
        .copy_from_slice(&filename_offset.to_le_bytes());
    bytes.extend_from_slice(texture_name);
    bytes.push(0);
    Ok(bytes)
}

fn skin_fixture() -> Result<Vec<u8>, Box<dyn Error>> {
    let skin = OldSkin {
        header: OldSkinHeader {
            bone_count_max: 1,
            ..OldSkinHeader::new()
        },
        indices: vec![0, 1, 2],
        triangles: vec![0, 1, 2],
        bone_indices: vec![0; 12],
        submeshes: vec![SkinSubmesh {
            id: 0,
            level: 0,
            vertex_start: 0,
            vertex_count: 3,
            triangle_start: 0,
            triangle_count: 3,
            bone_count: 0,
            bone_start: 0,
            bone_influence: 0,
            center: [0.0; 3],
            sort_center: [0.0; 3],
            bounding_radius: 1.0,
        }],
        batches: Vec::new(),
    };
    let mut cursor = Cursor::new(Vec::new());
    skin.write(&mut cursor)?;
    Ok(cursor.into_inner())
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, Box<dyn Error>> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or("fixture u32 is out of range")?
            .try_into()?,
    ))
}

fn world_model_client_fixture(files: &[(&str, &[u8])]) -> Result<ClientFixture, Box<dyn Error>> {
    let mut liquid = [0; 45];
    liquid[0] = 14;
    liquid[14] = 1;
    liquid[15] = 1;
    liquid[23] = 1f32.to_bits();
    liquid[25] = 1f32.to_bits();
    let mut strings = b"\0tileset\\fixture\\grass.blp\0".to_vec();
    liquid[16] = strings.len() as u32;
    strings.extend_from_slice(b"proceduralRiverDepthTex\0");
    let types = wdbc_fixture(&liquid, &strings);
    let materials = wdbc_fixture(&[1, 0, 1], &[0]);
    let texture = bootstrap_texture_blp();
    let mut complete = files.to_vec();
    complete.extend([
        ("DBFilesClient\\LiquidType.dbc", types.as_slice()),
        ("DBFilesClient\\LiquidMaterial.dbc", materials.as_slice()),
    ]);
    if !complete
        .iter()
        .any(|(path, _)| *path == "tileset\\fixture\\grass.blp")
    {
        complete.push(("tileset\\fixture\\grass.blp", texture.as_slice()));
    }
    ClientFixture::with_common_files(&complete)
}

fn root_wmo_fixture() -> Vec<u8> {
    let mut bytes = Vec::new();
    push_wmo_chunk(&mut bytes, *b"REVM", &17_u32.to_le_bytes());
    let mut header = vec![0_u8; 64];
    set_u32(&mut header, 0, 1);
    set_u32(&mut header, 4, 1);
    set_u32(&mut header, 16, 1);
    set_u32(&mut header, 20, 1);
    set_u32(&mut header, 24, 1);
    set_u32(&mut header, 32, 42);
    set_vec3(&mut header, 36, [-5.0, -5.0, -1.0]);
    set_vec3(&mut header, 48, [5.0, 5.0, 3.0]);
    set_u16(&mut header, 60, 0x4);
    push_wmo_chunk(&mut bytes, *b"DHOM", &header);
    push_wmo_chunk(&mut bytes, *b"XTOM", &[0]);
    let mut material = [0; 64];
    set_u32(&mut material, 28, u32::MAX);
    push_wmo_chunk(&mut bytes, *b"TMOM", &material);
    let mut group = Vec::new();
    group.extend_from_slice(&0_u32.to_le_bytes());
    for value in [-5.0_f32, -5.0, -1.0, 5.0, 5.0, 3.0] {
        group.extend_from_slice(&value.to_le_bytes());
    }
    group.extend_from_slice(&(-1_i32).to_le_bytes());
    push_wmo_chunk(&mut bytes, *b"IGOM", &group);
    push_wmo_chunk(&mut bytes, *b"NDOM", b"World\\Fixture\\Collision.mdx\0");
    let mut doodad = Vec::new();
    doodad.extend_from_slice(&0_u32.to_le_bytes());
    for value in [10.0_f32, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0] {
        doodad.extend_from_slice(&value.to_le_bytes());
    }
    doodad.extend_from_slice(&1.0_f32.to_le_bytes());
    doodad.extend_from_slice(&[u8::MAX; 4]);
    push_wmo_chunk(&mut bytes, *b"DDOM", &doodad);
    let mut set = [0_u8; 32];
    let name = b"Set_$DefaultGlobal";
    set[..name.len()].copy_from_slice(name);
    set_u32(&mut set, 24, 1);
    push_wmo_chunk(&mut bytes, *b"SDOM", &set);
    bytes
}

fn group_wmo_fixture() -> Vec<u8> {
    let mut nested = Vec::new();
    push_wmo_chunk(&mut nested, *b"YPOM", &[0x08, 0xff]);
    let mut indices = Vec::new();
    for index in [0_u16, 1, 2] {
        indices.extend_from_slice(&index.to_le_bytes());
    }
    push_wmo_chunk(&mut nested, *b"IVOM", &indices);
    let mut vertices = Vec::new();
    for vertex in [[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
        for value in vertex {
            vertices.extend_from_slice(&value.to_le_bytes());
        }
    }
    push_wmo_chunk(&mut nested, *b"TVOM", &vertices);
    let mut normals = Vec::new();
    for _ in 0..3 {
        for value in [0.0_f32, 0.0, 1.0] {
            normals.extend_from_slice(&value.to_le_bytes());
        }
    }
    push_wmo_chunk(&mut nested, *b"RNOM", &normals);
    let mut node = Vec::new();
    node.extend_from_slice(&4_u16.to_le_bytes());
    node.extend_from_slice(&(-1_i16).to_le_bytes());
    node.extend_from_slice(&(-1_i16).to_le_bytes());
    node.extend_from_slice(&1_u16.to_le_bytes());
    node.extend_from_slice(&0_u32.to_le_bytes());
    node.extend_from_slice(&0.0_f32.to_le_bytes());
    push_wmo_chunk(&mut nested, *b"NBOM", &node);
    push_wmo_chunk(&mut nested, *b"RBOM", &0_u16.to_le_bytes());
    let mut liquid = vec![0_u8; 30];
    set_u32(&mut liquid, 0, 2);
    set_u32(&mut liquid, 4, 2);
    set_u32(&mut liquid, 8, 1);
    set_u32(&mut liquid, 12, 1);
    for _ in 0..4 {
        liquid.extend_from_slice(&[0, 0, 0, 0]);
        liquid.extend_from_slice(&2.0_f32.to_le_bytes());
    }
    liquid.push(0x41);
    push_wmo_chunk(&mut nested, *b"QILM", &liquid);

    let mut group = vec![0_u8; 68];
    set_vec3(&mut group, 12, [-5.0, -5.0, -1.0]);
    set_vec3(&mut group, 24, [5.0, 5.0, 3.0]);
    set_u32(&mut group, 52, 2);
    group.extend_from_slice(&nested);
    let mut bytes = Vec::new();
    push_wmo_chunk(&mut bytes, *b"REVM", &17_u32.to_le_bytes());
    push_wmo_chunk(&mut bytes, *b"PGOM", &group);
    bytes
}

fn push_wmo_chunk(bytes: &mut Vec<u8>, magic: [u8; 4], payload: &[u8]) {
    bytes.extend_from_slice(&magic);
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(payload);
}

fn set_vec3(bytes: &mut [u8], offset: usize, value: [f32; 3]) {
    for (axis, value) in value.into_iter().enumerate() {
        set_f32(bytes, offset + axis * 4, value);
    }
}
