//! Real template decoding, native route clocks, and ECS placement integration.

#[path = "game_object_transports/collision.rs"]
mod collision;

use std::error::Error;
use std::rc::Rc;
use std::sync::Arc;

use glam::Vec3;
use solarity_asset::{
    AnimationDataCatalog, ArchiveCatalog, AssetStore, AssetStoreHandle, ClientDataRoot,
    GameObjectDisplayCatalog, Locale, TransportCatalog,
};
use solarity_ecs::{
    ActiveWorld, GameObjectMovement, GameObjectTransport, ObjectKind, WorldBootstrap, WorldMapId,
    WorldTransform,
};
use solarity_network::GameObjectQueryResponse;
use solarity_systems::project_object_fields;

use super::RuntimeGameObjectPresentation;
use crate::application::game_object_behavior::GameObjectNotification;
use crate::application::gameplay_coordinator::GameObjectTemplateCache;
use crate::random::CrtRand;
use crate::test_network::{TestError, WorldServer};
use crate::test_support::{ClientFixture, game_object_models as models};

/// The exact five-node native oracle route has period 8724 and station [3362, 5362).
fn presentation() -> Result<RuntimeGameObjectPresentation, Box<dyn Error>> {
    presentation_with_stations(&[2])
}

/// A second station distinguishes repeated-stop toggling from a no-op.
fn presentation_with_stations(
    stations: &[usize],
) -> Result<RuntimeGameObjectPresentation, Box<dyn Error>> {
    presentation_for_route(&route_nodes(stations))
}

/// Stock oracle controls reused by placement and live collision fixtures.
fn route_nodes(stations: &[usize]) -> Vec<Vec<u32>> {
    [
        [0., 0., 0.],
        [10., 0., 0.],
        [20., 5., 0.],
        [30., 0., 0.],
        [40., 0., 0.],
    ]
    .into_iter()
    .enumerate()
    .map(|(index, position): (usize, [f32; 3])| {
        vec![
            index as u32 + 1,
            42,
            index as u32,
            0,
            position[0].to_bits(),
            position[1].to_bits(),
            position[2].to_bits(),
            if stations.contains(&index) { 2 } else { 0 },
            if stations.contains(&index) { 2 } else { 0 },
            0,
            0,
        ]
    })
    .collect()
}

/// Mounts exact synthetic transport tables alongside a CPU-readable model.
fn presentation_for_route(
    nodes: &[Vec<u32>],
) -> Result<RuntimeGameObjectPresentation, Box<dyn Error>> {
    presentation_for_route_with_files(nodes, &[])
}

/// Extra map files exercise the same presentation owner against real residency.
fn presentation_for_route_with_files(
    nodes: &[Vec<u32>],
    files: &[(&str, &[u8])],
) -> Result<RuntimeGameObjectPresentation, Box<dyn Error>> {
    let paths = table(11, nodes);
    let physics = table(11, &[]);
    let keys = table(7, &[]);
    let model = collision::model()?;
    let skin = models::skin()?;
    let displays = models::displays();
    let mut sources: Vec<(&str, &[u8])> = vec![
        ("DBFilesClient\\TaxiPathNode.dbc", &paths),
        ("DBFilesClient\\TransportPhysics.dbc", &physics),
        ("DBFilesClient\\TransportAnimation.dbc", &keys),
        ("DBFilesClient\\TransportRotation.dbc", &keys),
        ("DBFilesClient\\GameObjectDisplayInfo.dbc", &displays),
        ("World\\GameObject.m2", &model),
        ("World\\GameObject00.skin", &skin),
    ];
    sources.extend_from_slice(files);
    let fixture = ClientFixture::with_common_files(&sources)?;
    let archive =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(archive)?;
    let catalog = TransportCatalog::load(&mut store)?;
    let displays = GameObjectDisplayCatalog::load(&mut store)?;
    let animations = Arc::new(AnimationDataCatalog::load(&mut store)?);
    Ok(
        RuntimeGameObjectPresentation::new(AssetStoreHandle::new(store), displays, animations)
            .with_transport_catalog(catalog),
    )
}

/// Include a real parent-local child to detect stale placement resolver caches.
fn world(period_ms: u32, progress: u16) -> Result<ActiveWorld, Box<dyn Error>> {
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        Vec3::ZERO,
        0.0,
    ));
    world.create_object(
        9,
        ObjectKind::GameObject,
        Some(WorldTransform::new(Vec3::splat(999.), 0.0)),
        [],
    )?;
    world.update_game_object_movement(
        9,
        GameObjectMovement::default().with_transport_clock(0, 100),
    )?;
    fields(
        &mut world,
        &[
            (3, 42),
            (4, 1_f32.to_bits()),
            (8, 42),
            (14, u32::from(progress) << 16),
            (16, period_ms),
            (17, 15 << 8),
        ],
    )?;
    world.create_object(
        10,
        ObjectKind::GameObject,
        Some(WorldTransform::new(Vec3::ZERO, 0.0)),
        [],
    )?;
    world.update_game_object_movement(
        10,
        GameObjectMovement::new(
            0,
            Some(GameObjectTransport {
                guid: 9,
                position: Vec3::new(2., 0., 1.),
                orientation: 0.,
            }),
        ),
    )?;
    project_object_fields(&mut world, 10, [(4, 1_f32.to_bits())])?;
    Ok(world)
}

fn fields(world: &mut ActiveWorld, fields: &[(u16, u32)]) -> Result<(), Box<dyn Error>> {
    world.update_fields(9, fields.iter().copied())?;
    project_object_fields(world, 9, fields.iter().copied())?;
    Ok(())
}

/// Decode through encrypted framing, so the production template has no test-only constructor.
fn template(allow_stopping: u32) -> Result<GameObjectQueryResponse, Box<dyn Error>> {
    let mut body: Vec<u8> = [42_u32, 15, 42]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    body.extend_from_slice(&[0; 7]);
    let mut properties = [0_u32; 24];
    properties[0] = 42;
    properties[1] = 10;
    properties[2] = 2;
    properties[8] = allow_stopping;
    body.extend(properties.into_iter().flat_map(u32::to_le_bytes));
    body.extend(1_f32.to_le_bytes());
    body.extend([0; 24]);
    let result: Result<_, TestError> = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let (server, mut session) = WorldServer::connect().await?;
            server.exchange(vec![(0x5f, body)], 0).await?.await??;
            session
                .receive_packet()
                .await?
                .game_object_query()?
                .ok_or("template reply missing".into())
        });
    result.map_err(|error| error as Box<dyn Error>)
}

fn table(columns: u32, rows: &[Vec<u32>]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    bytes.extend(
        [rows.len() as u32, columns, columns * 4, 1]
            .into_iter()
            .flat_map(u32::to_le_bytes),
    );
    bytes.extend(rows.iter().flatten().copied().flat_map(u32::to_le_bytes));
    bytes.push(0);
    bytes
}

#[test]
fn transport_admission_uses_running_clock_and_refreshes_parent_local_children()
-> Result<(), Box<dyn Error>> {
    let mut world = world(8724, u16::MAX)?;
    let mut objects = presentation()?;
    let mut cache = GameObjectTemplateCache::new();
    objects.synchronize(Some(&world))?;
    objects.synchronize_templates(&mut cache);
    objects.advance_transports(Some(&mut world), 100)?;
    assert!(world.game_object_animated_pose(9).is_none());
    cache.receive(template(0)?);
    let original = world.game_object_movement(9);
    // Late template admission samples receipt offset + current client time.
    objects.advance_transports(Some(&mut world), 3462)?;
    let pose = world.game_object_animated_pose(9).ok_or("boat pose")?;
    assert!((pose.matrix().w_axis.truncate() - Vec3::new(20., 5., 0.)).length() < 0.0001);
    let child = objects.object_placement(10).ok_or("child placement")?;
    assert!(
        (child.matrix().w_axis.truncate() - pose.matrix().transform_point3(Vec3::new(2., 0., 1.)))
            .length()
            < 0.0001
    );
    objects.advance_transports(Some(&mut world), 6100)?;
    assert_ne!(
        objects.object_placement(10).ok_or("moved child")?.matrix(),
        child.matrix()
    );
    assert_eq!(world.game_object_movement(9), original);
    assert_eq!(
        world.object_transform(9).ok_or("raw transform")?.position(),
        Vec3::splat(999.)
    );
    // Sparse LEVEL replication is retained but has no runtime watch/rebuild.
    fields(&mut world, &[(16, 0)])?;
    assert_eq!(
        world
            .game_object_presentation(9)
            .ok_or("fields")?
            .transport_period_ms(),
        0
    );
    objects.advance_transports(Some(&mut world), 8824)?;
    assert!(
        (world
            .game_object_animated_pose(9)
            .ok_or("wrapped")?
            .matrix()
            .w_axis
            .x
            - 10.)
            .abs()
            < 0.0001
    );
    Ok(())
}

#[test]
fn transport_repeated_stop_notification_releases_the_current_station() -> Result<(), Box<dyn Error>>
{
    let mut world = world(16000, 0)?;
    let mut objects = presentation_with_stations(&[2, 3])?;
    let mut cache = GameObjectTemplateCache::new();
    cache.receive(template(1)?);
    objects.synchronize(Some(&world))?;
    objects.synchronize_templates(&mut cache);
    objects.advance_transports(Some(&mut world), 100)?;
    objects.advance_transports(Some(&mut world), 3462)?;
    fields(&mut world, &[(17, (15 << 8) | 1)])?;
    let identity = world.object_identity(9).ok_or("identity")?;
    let mut random = CrtRand::new();
    objects.observe_notification(&world, identity, GameObjectNotification::State, &mut random)?;
    objects.advance_transports(Some(&mut world), 5462)?;
    let station = world.game_object_animated_pose(9).ok_or("station")?;
    objects.advance_transports(Some(&mut world), 6100)?;
    assert_eq!(world.game_object_animated_pose(9), Some(station));
    objects.observe_notification(&world, identity, GameObjectNotification::State, &mut random)?;
    objects.advance_transports(Some(&mut world), 6200)?;
    assert!(
        world
            .game_object_animated_pose(9)
            .ok_or("released")?
            .matrix()
            .w_axis
            .x
            > 20.
    );
    Ok(())
}

#[test]
fn transport_progress_is_admission_only_and_zero_server_period_does_not_fallback()
-> Result<(), Box<dyn Error>> {
    let mut world = world(8724, u16::MAX)?;
    let mut objects = presentation()?;
    let mut cache = GameObjectTemplateCache::new();
    cache.receive(template(1)?);
    objects.synchronize(Some(&world))?;
    objects.synchronize_templates(&mut cache);
    objects.advance_transports(Some(&mut world), 100)?;
    let start = world
        .game_object_animated_pose(9)
        .ok_or("initial full progress")?;
    assert!((start.matrix().w_axis.x - 10.).abs() < 0.0001);
    fields(&mut world, &[(14, (32768 << 16) | 0x10)])?;
    let identity = world.object_identity(9).ok_or("identity")?;
    objects.observe_notification(
        &world,
        identity,
        GameObjectNotification::Progress,
        &mut CrtRand::new(),
    )?;
    objects.advance_transports(Some(&mut world), 1100)?;
    let moving = world.game_object_animated_pose(9).ok_or("moving")?;
    assert!(moving.matrix().w_axis.x > 10. && moving.matrix().w_axis.x < 20.);
    // Reused GUIDs cannot inherit the old route/pose or its ignored LEVEL value.
    world.remove_object(9)?;
    objects.synchronize(Some(&world))?;
    world.create_object(
        9,
        ObjectKind::GameObject,
        Some(WorldTransform::new(Vec3::ZERO, 0.)),
        [],
    )?;
    world.update_game_object_movement(9, GameObjectMovement::default())?;
    fields(
        &mut world,
        &[
            (3, 42),
            (4, 1_f32.to_bits()),
            (8, 42),
            (16, 0),
            (17, 15 << 8),
        ],
    )?;
    objects.synchronize(Some(&world))?;
    objects.synchronize_templates(&mut cache);
    objects.advance_transports(Some(&mut world), 5000)?;
    assert!(world.game_object_animated_pose(9).is_none());
    Ok(())
}

#[test]
fn transport_model_completes_primary_phases_without_restarting_unchanged_routes()
-> Result<(), Box<dyn Error>> {
    let mut world = world(8724, 0)?;
    let mut objects = presentation()?;
    let mut cache = GameObjectTemplateCache::new();
    cache.receive(template(0)?);
    objects.synchronize(Some(&world))?;
    objects.synchronize_templates(&mut cache);
    objects.advance_transports(Some(&mut world), 6100)?;
    let identity = world.object_identity(9).ok_or("identity")?;
    let mut random = CrtRand::new();
    objects.synchronize_animations(Some(&world), &mut random)?;
    let transport = objects
        .movement_instance(identity)
        .ok_or("instance")?
        .transport_model()
        .ok_or("transport model")?;
    let playback = transport.playback().ok_or("playback")?;
    assert_eq!(playback.borrow().animation_id, 162);
    assert!(
        objects
            .movement_instance(identity)
            .ok_or("instance")?
            .behavior()
            .is_none()
    );
    objects
        .frame_input(Some(&world))
        .advance_scene(1001., 1001., &mut random)?;
    assert_eq!(playback.borrow().animation_id, 163);
    let sample = transport.take_scene_sample().ok_or("completion")?;
    assert_eq!(sample.advance.expired_variations.len(), 1);
    assert!(transport.take_scene_sample().is_none());
    let timer = playback.borrow().script_timer;
    objects.synchronize_animations(Some(&world), &mut random)?;
    assert_eq!(playback.borrow().animation_id, 163);
    assert_eq!(playback.borrow().script_timer, timer);
    assert!(Rc::ptr_eq(
        &playback,
        &transport.playback().ok_or("retained timer")?
    ));

    // The route's next deceleration requests ShipStop, whose primary completion is Stand.
    objects.advance_transports(Some(&mut world), 9824)?;
    objects.synchronize_animations(Some(&world), &mut random)?;
    assert_eq!(playback.borrow().animation_id, 164);
    objects
        .frame_input(Some(&world))
        .advance_scene(2002., 2002., &mut random)?;
    assert_eq!(playback.borrow().animation_id, 0);
    objects.synchronize_animations(Some(&world), &mut random)?;
    assert_eq!(playback.borrow().animation_id, 0);
    Ok(())
}

#[test]
fn transport_next_map_section_retains_the_current_pose_until_server_transfer()
-> Result<(), Box<dyn Error>> {
    let nodes: Vec<_> = (0..8)
        .map(|index| {
            vec![
                index + 1,
                42,
                index,
                u32::from(index >= 4),
                ((index % 4) as f32 * 10.).to_bits(),
                0,
                0,
                0,
                0,
                0,
                0,
            ]
        })
        .collect();
    // Each section traverses the middle ten yards at ten yards per second.
    let mut world = world(2000, 0)?;
    let mut objects = presentation_for_route(&nodes)?;
    let mut cache = GameObjectTemplateCache::new();
    cache.receive(template(0)?);
    objects.synchronize(Some(&world))?;
    objects.synchronize_templates(&mut cache);
    objects.advance_transports(Some(&mut world), 600)?;
    let current = world
        .game_object_animated_pose(9)
        .ok_or("current map pose")?;
    objects.advance_transports(Some(&mut world), 1600)?;
    assert_eq!(world.game_object_animated_pose(9), Some(current));
    assert_eq!(world.map_id().value(), 0);
    objects.advance_transports(Some(&mut world), 2100)?;
    assert_ne!(world.game_object_animated_pose(9), Some(current));
    Ok(())
}
