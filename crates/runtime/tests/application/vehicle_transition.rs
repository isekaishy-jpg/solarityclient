//! Passenger execution survives model replacement and finishes its exit after unlinking.

use super::super::UnitAnimationInput;
use super::*;
use crate::random::CrtRand;
use solarity_asset::{
    AnimationDataCatalog, ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, Locale,
};
use solarity_ecs::{
    UnitAnimationTier, WorldBootstrap, WorldMapId, WorldMovementContext, WorldMovementSpeeds,
    WorldMovementState, WorldMovementTransport,
};
use std::{error::Error, sync::Arc};

type TestResult = Result<(), Box<dyn Error>>;

fn movement(parent: u64, special_exit: bool) -> WorldMovementState {
    WorldMovementState::new(
        if special_exit { 0x40 << 32 } else { 0 },
        WorldMovementSpeeds::new([0.; 9]),
        WorldMovementContext {
            transport: (parent != 0).then_some(WorldMovementTransport {
                guid: parent,
                position: Vec3::ZERO,
                orientation: 0.,
                time_ms: 0,
                seat: 2,
                interpolated_time_ms: None,
            }),
            ..Default::default()
        },
    )
}

fn event(
    identity: WorldObjectIdentity,
    previous: u64,
    parent: Option<WorldObjectIdentity>,
    animated: bool,
    special_exit: bool,
) -> UnitMovementAnimationEvent {
    UnitMovementAnimationEvent {
        identity,
        movement: movement(parent.map_or(0, |parent| parent.guid()), special_exit),
        stand: 0,
        kind: UnitMovementAnimationEventKind::Passenger {
            previous_transform: WorldTransform::new(Vec3::X * 10., 0.),
            previous: (previous != 0).then_some((previous, 2)),
            parent,
            animated,
        },
    }
}

#[test]
fn entry_and_exit_keep_the_unit_controller_across_model_replacement() -> TestResult {
    let fixture = crate::test_support::unit_models::fixture_with_vehicle_seats()?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let vehicles = Arc::new(VehicleCatalog::load(&mut store)?);
    let animations = Arc::new(AnimationDataCatalog::load(&mut store)?);
    let model = Arc::new(DecodedM2Model::load(
        &mut store,
        &AssetPath::new("Character/Human/Male/HumanMale.m2")?,
    )?);
    let replacement = Arc::new(DecodedM2Model::load(
        &mut store,
        &AssetPath::new("Creature/Alternate.m2")?,
    )?);
    let frames = UnitPassengerFrames::new(Arc::clone(&vehicles));
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Passenger",
        Vec3::X * 2.,
        0.,
    ));
    world.create_object(
        9,
        ObjectKind::Unit,
        Some(WorldTransform::new(Vec3::ZERO, 0.)),
        [(4, 1_f32.to_bits())],
    )?;
    world.set_unit_vehicle(9, 1, 0.);
    let child = world.object_identity(7).ok_or("child")?;
    let parent = world.object_identity(9).ok_or("parent")?;
    let mut scene = UnitAnimationScene::default();
    scene.set_scene_time(100);
    world.update_movement(7, movement(9, false))?;
    scene.notify_movement(event(child, 0, Some(parent), true, false));
    scene.synchronize_passengers(&world, &vehicles, &frames);
    let state = scene.passenger_state(child);
    assert_eq!(state.borrow().phase, Phase::EnterDelay);
    assert_eq!(state.borrow().timing.ok_or("delay timer")?.end_ms(), 350);
    let input = UnitAnimationInput::new(
        0,
        UnitAnimationTier::Ground,
        false,
        Some(movement(9, false)),
    );
    scene.bind(child, &model, &animations, input);
    let first = Rc::clone(scene.get(7).ok_or("first model")?);
    assert!(Rc::ptr_eq(&state, &first.passenger));
    let mut random = CrtRand::new();
    first.advance_scene(100., &mut random)?;
    assert_eq!(first.playback.borrow().animation_id, 96);
    first.publish_passenger_transform(
        first
            .passenger_transition_transform(100, Vec3::X * 2., 1.)
            .ok_or("held origin")?,
    );
    first.advance_passenger(350, Vec3::X * 2., Vec3::ZERO);
    assert_eq!(first.passenger_phase(), Phase::Entering);
    assert_eq!(state.borrow().timing.ok_or("travel timer")?.end_ms(), 2350);
    first.advance_scene(1101., &mut random)?;
    assert_eq!(first.passenger_transition_animation(), Some(91));
    assert_eq!(first.playback.borrow().animation_id, 91);
    let halfway = first
        .passenger_transition_transform(1350, Vec3::X * 2., 1.)
        .ok_or("entry pose")?;
    assert!(
        halfway
            .w_axis
            .truncate()
            .abs_diff_eq(Vec3::new(6., 0., 10.), 0.000002)
    );
    first.publish_passenger_transform(halfway);
    scene.set_scene_time(1350);
    scene.bind(child, &replacement, &animations, input);
    let second = Rc::clone(scene.get(7).ok_or("replacement")?);
    assert!(!Rc::ptr_eq(&first, &second));
    assert!(Rc::ptr_eq(&first.passenger, &second.passenger));
    assert_eq!(
        second.passenger_transition_transform(1350, Vec3::X * 2., 1.),
        Some(halfway)
    );
    second.advance_passenger(2350, Vec3::X * 2., Vec3::ZERO);
    assert_eq!(second.passenger_phase(), Phase::Seated);
    second.publish_passenger_transform(Mat4::from_translation(Vec3::X * 2.));
    scene.set_scene_time(2400);
    world.update_movement(7, movement(0, false))?;
    world.update_transform(7, WorldTransform::new(Vec3::X * 6., 0.))?;
    scene.notify_movement(event(child, 9, None, true, false));
    scene.synchronize_passengers(&world, &vehicles, &frames);
    assert_eq!(second.passenger_phase(), Phase::ExitDelay);
    assert_eq!(
        second.passenger_pose_input().map(|input| input.parent),
        Some(parent)
    );
    second.advance_passenger(2400, Vec3::X * 6., Vec3::ZERO);
    second.advance_passenger(2525, Vec3::X * 6., Vec3::ZERO);
    assert_eq!(second.passenger_phase(), Phase::Exiting);
    let exiting = second
        .passenger_transition_transform(2775, Vec3::ZERO, 1.)
        .ok_or("exit pose")?;
    assert!(
        exiting
            .w_axis
            .truncate()
            .abs_diff_eq(Vec3::new(4., 0., 0.625), 0.000002)
    );
    second.advance_passenger(3025, Vec3::X * 6., Vec3::ZERO);
    assert_eq!(second.passenger_phase(), Phase::Detached);
    assert!(second.passenger_input().is_none());
    Ok(())
}

#[test]
fn immediate_admission_and_special_exit_obey_seat_flags() -> TestResult {
    let fixture = crate::test_support::unit_models::fixture_with_vehicle_seats()?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let vehicles = Arc::new(VehicleCatalog::load(&mut store)?);
    let frames = UnitPassengerFrames::new(Arc::clone(&vehicles));
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Passenger",
        Vec3::ZERO,
        0.,
    ));
    world.create_object(
        9,
        ObjectKind::Unit,
        Some(WorldTransform::new(Vec3::ZERO, 0.)),
        [(4, 1_f32.to_bits())],
    )?;
    world.set_unit_vehicle(9, 1, 0.);
    let child = world.object_identity(7).ok_or("child")?;
    let parent = world.object_identity(9).ok_or("parent")?;
    let scene = UnitAnimationScene::default();
    world.update_movement(7, movement(9, false))?;
    scene.notify_movement(event(child, 0, Some(parent), false, false));
    scene.synchronize_passengers(&world, &vehicles, &frames);
    assert_eq!(scene.passenger_state(child).borrow().phase, Phase::Seated);
    world.update_movement(7, movement(0, true))?;
    scene.notify_movement(event(child, 9, None, true, true));
    scene.synchronize_passengers(&world, &vehicles, &frames);
    assert_eq!(
        scene.passenger_state(child).borrow().phase,
        Phase::Detached,
        "secondary movement bit 40 selects seat flag 8, not ordinary exit flag 8000"
    );
    Ok(())
}

#[test]
fn unloaded_controller_advances_and_guid_reuse_starts_a_fresh_lifetime() -> TestResult {
    let fixture = crate::test_support::unit_models::fixture_with_vehicle_seats()?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let vehicles = Arc::new(VehicleCatalog::load(&mut store)?);
    let frames = UnitPassengerFrames::new(Arc::clone(&vehicles));
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Passenger",
        Vec3::ZERO,
        0.,
    ));
    for guid in [8, 9] {
        world.create_object(
            guid,
            ObjectKind::Unit,
            Some(WorldTransform::new(Vec3::X * 2., 0.)),
            [(4, 1_f32.to_bits())],
        )?;
    }
    world.set_unit_vehicle(9, 1, 0.);
    let child = world.object_identity(8).ok_or("child")?;
    let parent = world.object_identity(9).ok_or("parent")?;
    let mut scene = UnitAnimationScene::default();
    scene.set_scene_time(100);
    world.update_movement(8, movement(9, false))?;
    scene.notify_movement(event(child, 0, Some(parent), true, false));
    let state = scene.passenger_state(child);
    for (now, phase) in [
        (100, Phase::EnterDelay),
        (349, Phase::EnterDelay),
        // With neither model loaded, phase entry and target both resolve the
        // unit position. The zero-distance travel settles at the delay boundary.
        (350, Phase::Seated),
    ] {
        scene.set_scene_time(now);
        scene.synchronize_passengers(&world, &vehicles, &frames);
        assert_eq!(state.borrow().phase, phase);
        assert!(scene.get(8).is_none(), "no child model has loaded");
        assert!(
            scene.get(9).is_none(),
            "missing parent model selects unit-position target"
        );
    }
    world.remove_object(8)?;
    world.create_object(
        8,
        ObjectKind::Unit,
        Some(WorldTransform::new(Vec3::ZERO, 0.)),
        [(4, 1_f32.to_bits())],
    )?;
    let replacement = world.object_identity(8).ok_or("replacement")?;
    scene.retain_world(&world);
    let next = scene.passenger_state(replacement);
    assert!(!Rc::ptr_eq(&state, &next));
    assert_eq!(next.borrow().phase, Phase::Detached);
    assert!(next.borrow().timing.is_none());
    Ok(())
}
