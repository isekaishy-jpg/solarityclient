//! Unit/vehicle ancestry through live geometry and movement publication.

use super::*;
use crate::application::player_movement::{RuntimePlayerMovement, remote::RuntimeRemoteMovement};
use crate::application::unit_passenger::UnitPassengerFrames;
use crate::application::{
    RuntimeGameObjectPresentation, RuntimeMovementGeometry, RuntimeMovementQuery,
    RuntimePlayerCatalogs, RuntimePlayerItemCatalogs, RuntimePlayerPresentation,
    RuntimeTerrainCoordinator,
};
use solarity_asset::{
    AnimationDataCatalog, ArchiveCatalog, AssetStore, AssetStoreHandle, CharacterAppearanceCatalog,
    CharacterRaceCatalog, CharacterStartOutfitCatalog, ClientDataRoot, CreatureCatalog,
    CreatureFamilyCatalog, GameObjectDisplayCatalog, HelmetGeosetVisibilityCatalog,
    ItemDefinitionCatalog, ItemDisplayCatalog, ItemVisualCatalog, LiquidTypeCatalog, Locale,
    MapCatalog, ParticleColorCatalog, VehicleCatalog,
};
use solarity_ecs::{
    ObjectKind, WorldMovementContext, WorldMovementSpeeds, WorldMovementState,
    WorldMovementTransport, WorldTransform,
};
use solarity_systems::{MovementBspCacheMode, MovementTransportFrame};
use std::{error::Error, sync::Arc};

type TestResult = Result<(), Box<dyn Error>>;

fn vehicle_catalog() -> Result<VehicleCatalog, Box<dyn Error>> {
    let fixture = crate::test_support::unit_models::fixture()?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    Ok(VehicleCatalog::load(&mut store)?)
}

fn speeds() -> WorldMovementSpeeds {
    WorldMovementSpeeds::new([
        2.5,
        7.,
        4.5,
        4.72,
        2.5,
        7.,
        4.5,
        std::f32::consts::PI,
        std::f32::consts::PI,
    ])
}

fn movement(parent: u64, local: Vec3, yaw: f32) -> WorldMovementState {
    WorldMovementState::new(
        if parent == 0 { 0 } else { 0x200 },
        speeds(),
        WorldMovementContext {
            transport: (parent != 0).then_some(WorldMovementTransport {
                guid: parent,
                position: local,
                orientation: yaw,
                time_ms: 987,
                interpolated_time_ms: Some(654),
                seat: 2,
            }),
            ..Default::default()
        },
    )
}

fn unit(
    world: &mut ActiveWorld,
    guid: u64,
    pose: WorldTransform,
    parent: u64,
    local: Vec3,
    yaw: f32,
) -> TestResult {
    world.create_object(guid, ObjectKind::Unit, Some(pose), [(4, 3_f32.to_bits())])?;
    world.update_movement(guid, movement(parent, local, yaw))?;
    Ok(())
}

#[test]
fn unit_frames_compose_nested_units_with_a_live_game_object_and_ignore_model_scale() -> TestResult {
    let mut scene = crate::application::game_object_coordinator::transport_tests::collision::Scene::passenger_deck()?;
    let frames = UnitPassengerFrames::new(Arc::new(vehicle_catalog()?));
    let local = Vec3::new(2., -3., 1.);
    unit(
        &mut scene.world,
        20,
        WorldTransform::new(Vec3::ZERO, 0.),
        9,
        local,
        0.3,
    )?;
    scene.world.set_unit_vehicle(20, 1, -1.5);
    unit(
        &mut scene.world,
        21,
        WorldTransform::new(Vec3::ZERO, 0.),
        20,
        Vec3::X,
        -0.7,
    )?;
    frames.synchronize(Some(&scene.world), &scene.objects)?;
    for time in [4000, 4300] {
        scene.synchronize(time)?;
        let root = scene
            .objects
            .object_movement_frame(scene.world.object_identity(9).ok_or("root")?)?
            .ok_or("root frame")?;
        let parent = MovementTransportFrame::unit(local, 0.3, Some(root))?;
        let expected = MovementTransportFrame::unit(Vec3::X, -0.7, Some(parent))?;
        let identity = scene.world.object_identity(21).ok_or("child")?;
        let actual = frames
            .resolve(&scene.world, &scene.objects, identity)?
            .ok_or("child frame")?;
        assert_eq!(actual.world_matrix(), expected.world_matrix());
        assert_eq!(actual.facing(), expected.facing());
        let mut query = RuntimeMovementQuery::new();
        let geometry = RuntimeMovementGeometry::new(
            &mut scene.terrain,
            &scene.world,
            &scene.objects,
            0x8010_8111,
            MovementBspCacheMode::Enabled,
            &mut query,
        )
        .with_unit_parents(&frames);
        assert!(geometry.can_board(identity));
        assert!(geometry.retains_passenger(identity, Vec3::splat(100_000.))?);
        assert_eq!(
            geometry.passenger_time_ms(identity),
            None,
            "Unit +F4 does not publish a GO clock"
        );
        assert_eq!(
            geometry
                .passenger_frame(identity)?
                .ok_or("geometry frame")?
                .world_matrix(),
            expected.world_matrix()
        );
    }
    Ok(())
}

#[test]
fn vehicle_matrix_retention_distinguishes_missing_rows_parents_and_reused_generations() -> TestResult
{
    let mut scene = crate::application::game_object_coordinator::transport_tests::collision::Scene::passenger_deck()?;
    let frames = UnitPassengerFrames::new(Arc::new(vehicle_catalog()?));
    let initial = WorldTransform::new(Vec3::new(70., 80., 90.), 1.);
    for (guid, definition) in [(20, 1), (21, 999999)] {
        unit(&mut scene.world, guid, initial, 99, Vec3::X, -0.2)?;
        scene.world.set_unit_vehicle(guid, definition, -2.);
    }
    // More network publications may arrive before the first presentation pass.
    scene
        .world
        .update_transform(20, WorldTransform::new(Vec3::splat(-500.), 2.))?;
    scene.world.set_unit_vehicle(20, 1, 2.5);
    frames.synchronize(Some(&scene.world), &scene.objects)?;
    let valid = scene.world.object_identity(20).ok_or("vehicle")?;
    let missing = scene.world.object_identity(21).ok_or("missing row")?;
    let retained = frames
        .resolve(&scene.world, &scene.objects, valid)?
        .ok_or("cached matrix")?;
    assert_eq!(
        retained.world_matrix(),
        MovementTransportFrame::unit(initial.position(), initial.orientation(), None)?
            .world_matrix()
    );
    assert_eq!(
        retained.facing(),
        MovementTransportFrame::new(glam::Mat4::IDENTITY, 0.)?.world_orientation(-0.2)
    );
    assert!(
        frames
            .resolve(&scene.world, &scene.objects, missing)?
            .is_none()
    );
    unit(
        &mut scene.world,
        99,
        WorldTransform::new(Vec3::new(5., 6., 7.), 0.5),
        0,
        Vec3::ZERO,
        0.,
    )?;
    let linked = frames
        .resolve(&scene.world, &scene.objects, valid)?
        .ok_or("late parent")?;
    assert_ne!(linked.world_matrix(), retained.world_matrix());
    let old_parent = scene.world.object_identity(99).ok_or("parent")?;
    scene.world.remove_object(99)?;
    unit(
        &mut scene.world,
        99,
        WorldTransform::new(Vec3::splat(9000.), 2.),
        0,
        Vec3::ZERO,
        0.,
    )?;
    assert_ne!(scene.world.object_identity(99), Some(old_parent));
    let unchanged = frames
        .resolve(&scene.world, &scene.objects, valid)?
        .ok_or("old cache")?;
    assert_eq!(
        unchanged.world_matrix(),
        linked.world_matrix(),
        "reused GUID must not replace an admitted parent"
    );
    scene
        .world
        .update_movement(20, movement(0, Vec3::ZERO, 0.))?;
    frames.resolve(&scene.world, &scene.objects, valid)?;
    scene
        .world
        .update_movement(20, movement(99, Vec3::X, -0.2))?;
    assert_ne!(
        frames
            .resolve(&scene.world, &scene.objects, valid)?
            .ok_or("new link")?
            .world_matrix(),
        linked.world_matrix()
    );
    scene.world.remove_object(20)?;
    frames.synchronize(Some(&scene.world), &scene.objects)?;
    assert!(
        frames
            .resolve(&scene.world, &scene.objects, valid)?
            .is_none()
    );
    unit(&mut scene.world, 20, initial, 20, Vec3::ZERO, 0.)?;
    assert!(matches!(
        frames.resolve(
            &scene.world,
            &scene.objects,
            scene.world.object_identity(20).ok_or("new unit")?
        ),
        Err(crate::application::RuntimePlayerMovementError::PassengerCycle)
    ));
    Ok(())
}

#[test]
fn local_and_remote_passengers_follow_a_later_parent_without_restarting_local_motion() -> TestResult
{
    let fixture = super::movement_entry_tests::flat_world()?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let maps = MapCatalog::load(&mut store)?;
    let displays = GameObjectDisplayCatalog::load(&mut store)?;
    let liquids = LiquidTypeCatalog::load(&mut store)?;
    let animations = Arc::new(AnimationDataCatalog::load(&mut store)?);
    let catalogs = RuntimePlayerCatalogs::new(
        animations.clone(),
        CreatureCatalog::load(&mut store)?,
        CreatureFamilyCatalog::default(),
        CharacterAppearanceCatalog::load(&mut store)?,
        CharacterRaceCatalog::load(&mut store)?,
        HelmetGeosetVisibilityCatalog::load(&mut store)?,
        CharacterStartOutfitCatalog::load(&mut store)?,
        RuntimePlayerItemCatalogs::new(
            ItemDefinitionCatalog::load(&mut store)?,
            ItemDisplayCatalog::load(&mut store)?,
            ItemVisualCatalog::load(&mut store)?,
        ),
        ParticleColorCatalog::load(&mut store)?,
    )
    .with_vehicles(vehicle_catalog()?);
    let assets = AssetStoreHandle::new(store);
    let mut terrain = RuntimeTerrainCoordinator::new(assets.clone(), maps);
    let objects = RuntimeGameObjectPresentation::new(assets.clone(), displays, animations);
    let presentation = RuntimePlayerPresentation::new(assets, catalogs);
    let origin = Vec3::new(1000., 5800., 10.);
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        1,
        "Passenger",
        origin,
        0.,
    ));
    for (guid, parent, local) in [
        (50, 0, Vec3::ZERO),
        (20, 50, Vec3::X * 2.),
        (10, 20, Vec3::X * 3.),
        (11, 20, Vec3::X * 4.),
    ] {
        unit(
            &mut world,
            guid,
            WorldTransform::new(origin, 0.),
            parent,
            local,
            0.,
        )?;
    }
    world.set_unit_vehicle(50, 1, 0.);
    world.set_unit_vehicle(20, 1, 0.);
    world.update_movement(50, movement(0, Vec3::ZERO, 0.).with_flags(0x11))?;
    world.update_movement(11, movement(20, Vec3::X * 4., 0.).with_flags(0x0200_0200))?;
    world.update_movement(1, movement(20, Vec3::Y * 2., 0.))?;
    let entity = world.local_player();
    world.storage_mut().add_component(
        entity,
        (solarity_ecs::UnitVitals::new(100, 100, [0; 7], [0; 7]),),
    );
    let runtime = tokio::runtime::Builder::new_current_thread().build()?;
    let (_packets, receiver) = mpsc::channel(8);
    let (commands, mut writer) = mpsc::channel(128);
    let mut gameplay = RuntimeGameplayCoordinator::with_test_world(world);
    gameplay.active = Some(ActiveGameplayNetwork {
        receiver,
        commands,
        task: runtime.spawn(std::future::pending()),
    });
    let mut local = RuntimePlayerMovement::default();
    let mut remote = RuntimeRemoteMovement::default();
    terrain.synchronize(gameplay.world())?;
    terrain.synchronize_game_object_movement(
        gameplay.world(),
        &objects,
        MovementBspCacheMode::Enabled,
    )?;
    let mut previous_parent_x = origin.x;
    for now in [0, 100, 200, 400] {
        if now == 100 {
            // Left drag holds world camera yaw while the vehicle turns underneath.
            local.push(solarity_ui::UiMovementCommand {
                action: solarity_ui::UiMovementAction::Hold {
                    control: solarity_ui::UiMovementControl::CameraOrSelectOrMove,
                    pressed: true,
                },
                timestamp_ms: now,
            });
        }
        presentation
            .passenger_frames
            .synchronize(gameplay.world(), &objects)?;
        local.service(
            &mut gameplay,
            &mut terrain,
            &objects,
            &liquids,
            &presentation.passenger_frames,
            presentation.movement_animations(),
            Some([0.5, 2., 1.]),
            now,
        )?;
        remote.service(
            &gameplay,
            &mut terrain,
            &objects,
            &presentation,
            &liquids,
            now,
        )?;
        let before_world = gameplay.world().ok_or("world")?;
        let camera_yaw = before_world.local_player_transform()?.orientation()
            + before_world.local_player_view()?.yaw_offset_radians();
        local.refresh_passenger_projection(gameplay.world(), &objects, &presentation)?;
        let world = gameplay.world().ok_or("world")?;
        let parent_pose = world.object_transform(50).ok_or("parent")?;
        let parent = parent_pose.position();
        let parent_frame = MovementTransportFrame::unit(parent, parent_pose.orientation(), None)?;
        let final_camera_yaw = world.local_player_transform()?.orientation()
            + world.local_player_view()?.yaw_offset_radians();
        assert!((camera_yaw.cos() - final_camera_yaw.cos()).abs() < 0.00001);
        assert!((camera_yaw.sin() - final_camera_yaw.sin()).abs() < 0.00001);
        if now != 0 {
            assert!(
                parent.x > previous_parent_x,
                "parent must actually advance: {parent:?}"
            );
            assert!(parent_pose.orientation() > 0., "parent must actually turn");
        }
        previous_parent_x = parent.x;
        for (guid, offset, parent_guid) in [
            (1, Vec3::new(2., 2., 0.), 20),
            (10, Vec3::new(5., 0., 0.), 20),
            (11, Vec3::new(6., 0., 0.), 20),
            (20, Vec3::new(2., 0., 0.), 50),
        ] {
            let position = world.object_transform(guid).ok_or("passenger")?.position();
            assert!(
                (position - parent_frame.world_position(offset)).length() < 0.02,
                "{now}/{guid}: {position:?} parent {parent:?}"
            );
            assert!(
                (world
                    .object_transform(guid)
                    .ok_or("passenger")?
                    .orientation()
                    - parent_pose.orientation())
                .abs()
                    < 0.00001
            );
            let state = world.movement_state(guid).ok_or("state")?;
            assert_eq!(state.transport_guid(), Some(parent_guid));
            let transport = state.context().transport.ok_or("transport")?;
            assert_eq!(transport.seat, 2);
            if guid != 1 {
                assert_eq!(
                    (transport.time_ms, transport.interpolated_time_ms),
                    (987, Some(654))
                );
            }
        }
        let before = world.movement_state(1).ok_or("local")?;
        local.refresh_passenger_projection(Some(world), &objects, &presentation)?;
        assert_eq!(
            world.movement_state(1),
            Some(before),
            "projection must not advance clocks or analytic anchors"
        );
        while writer.try_recv().is_ok() {}
    }
    // Destruction and an authoritative reattachment can share a receive batch.
    // A newly admitted owner must replace the old ancestor identity explicitly.
    let replacement_origin = origin + Vec3::X * 40.;
    let world = gameplay.world.as_mut().ok_or("world")?;
    world.remove_object(50)?;
    unit(
        world,
        50,
        WorldTransform::new(replacement_origin, 0.),
        0,
        Vec3::ZERO,
        0.,
    )?;
    world.set_unit_vehicle(50, 1, 0.);
    world.update_transform(
        20,
        WorldTransform::new(replacement_origin + Vec3::X * 2., 0.),
    )?;
    let mut correction = movement(50, Vec3::X * 2., 0.).context();
    correction.timestamp_ms = 450;
    world.update_movement(20, WorldMovementState::new(0x200, speeds(), correction))?;
    presentation
        .passenger_frames
        .synchronize(gameplay.world(), &objects)?;
    remote.service(
        &gameplay,
        &mut terrain,
        &objects,
        &presentation,
        &liquids,
        450,
    )?;
    local.refresh_passenger_projection(gameplay.world(), &objects, &presentation)?;
    let world = gameplay.world().ok_or("world")?;
    for (guid, offset) in [
        (1, Vec3::new(2., 2., 0.)),
        (10, Vec3::X * 5.),
        (20, Vec3::X * 2.),
    ] {
        let position = world
            .object_transform(guid)
            .ok_or("reattached passenger")?
            .position();
        assert!(
            (position - replacement_origin - offset).length() < 0.02,
            "reattached {guid}: {position:?}"
        );
    }
    Ok(())
}
