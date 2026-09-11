//! Vehicle-owned model clips use the original owner lifetime and completion phase.

use super::*;

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn vehicle_owner_lifetime_ends_on_world_retirement_even_with_a_retained_model() -> TestResult {
    let fixture = vehicle_owner()?;
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        9,
        "Vehicle",
        Vec3::ZERO,
        0.,
    ));
    world.create_object(
        7,
        solarity_ecs::ObjectKind::Unit,
        Some(solarity_ecs::WorldTransform::new(Vec3::ZERO, 0.)),
        [],
    )?;
    let mut scene = UnitAnimationScene::default();
    scene.bind(
        world.object_identity(7).ok_or("passenger identity")?,
        &fixture.model,
        &fixture.animations,
        input(0),
    );
    let passenger = Rc::clone(scene.get(7).ok_or("passenger owner")?);
    let token = passenger
        .passenger_controller()
        .vehicle_animation_lifetime();
    let mut random = CrtRand::new();
    fixture.advance_scene(100., &mut random)?;
    fixture.start_vehicle_ride_animation(7, &token, 4, 115, 110, &mut random)?;
    world.remove_object(7)?;
    scene.retain_world(&world);
    assert!(token.get());
    assert_eq!(
        Rc::strong_count(&passenger),
        1,
        "retired model can still retain its passenger state"
    );
    fixture.advance_scene(370., &mut random)?;
    assert!(!fixture.vehicle_controls_body_key(4));
    assert!(
        fixture
            .playback
            .borrow()
            .bone_playback(4)
            .ok_or("released upper")?
            .script_timer
            .is_none()
    );
    Ok(())
}

#[test]
fn vehicle_key_alias_interrupts_the_actual_root_callback() -> TestResult {
    let owner = owner_with_input(&[0, 115, 116], input(0))?;
    let passenger = Rc::new(Cell::new(false));
    let mut random = CrtRand::new();
    owner.advance_scene(100., &mut random)?;
    owner.start_vehicle_ride_animation(7, &passenger, -1, 115, 110, &mut random)?;
    assert!(owner.vehicle_controls_body_key(-1));
    // This model maps semantic key 4 onto bone zero. Its interrupted callback
    // reports -1, while Vehicle_C still records the newly requested key 4.
    owner.start_vehicle_ride_animation(8, &passenger, 4, 116, 120, &mut random)?;
    assert!(!owner.vehicle_controls_body_key(-1));
    assert!(owner.vehicle_controls_body_key(4));
    assert_eq!(owner.playback.borrow().animation_id, 116);
    Ok(())
}

fn vehicle_owner() -> Result<UnitAnimationBehavior, Box<dyn Error>> {
    let fixture = crate::test_support::unit_models::fixture_with_vehicle_ride_animation(115, 4)?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let model = Arc::new(DecodedM2Model::load(
        &mut store,
        &AssetPath::new("Character/Human/Male/HumanMale.m2")?,
    )?);
    let animations = Arc::new(AnimationDataCatalog::load(&mut store)?);
    let world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        9,
        "Vehicle",
        Vec3::ZERO,
        0.,
    ));
    Ok(UnitAnimationBehavior::new(
        world.object_identity(9).ok_or("vehicle identity")?,
        model,
        animations,
        input(0),
        100,
    ))
}

#[test]
fn vehicle_ride_completion_replays_with_overdue_phase_until_last_passenger_leaves() -> TestResult {
    let owner = vehicle_owner()?;
    let first = Rc::new(Cell::new(false));
    let second = Rc::new(Cell::new(false));
    let mut random = CrtRand::new();
    owner.advance_scene(100., &mut random)?;
    let body_timer = owner.playback.borrow().script_timer;
    assert!(owner.start_vehicle_ride_animation(7, &first, 4, 115, 110, &mut random)?);
    assert!(owner.start_vehicle_ride_animation(8, &second, 4, 115, 120, &mut random)?);
    let mut expected_random = random;
    for _ in 0..2 {
        let _ = expected_random.next_u15();
    }
    owner.advance_scene(370., &mut random)?;
    assert_eq!(
        random, expected_random,
        "owned completion selects a variation and cycle count"
    );
    {
        let playback = owner.playback.borrow();
        assert_eq!(playback.script_timer, body_timer);
        let upper = playback.bone_playback(4).ok_or("vehicle upper")?;
        assert_eq!(upper.animation_id, 115);
        // Start 121 + authored duration 200 = 321, observed at 370. Replay
        // starts at 370 with offset 49, unlike ordinary mount completion.
        assert_eq!(
            upper
                .script_timer
                .ok_or("replayed upper")?
                .unwrapped_time(370),
            49
        );
    }
    assert!(owner.vehicle_controls_body_key(4));
    let before_release = random;
    owner.release_vehicle_ride_animation(7, 4, 380, &mut random)?;
    assert_eq!(random, before_release);
    assert!(owner.vehicle_controls_body_key(4));
    owner.release_vehicle_ride_animation(8, 4, 390, &mut random)?;
    assert!(!owner.vehicle_controls_body_key(4));
    let playback = owner.playback.borrow();
    let upper = playback.bone_playback(4).ok_or("released upper")?;
    assert!(upper.script_timer.is_none());
    assert!(upper.script_blend.is_some());
    assert_eq!(playback.script_timer, body_timer);
    Ok(())
}

#[test]
fn vehicle_root_interruption_releases_control_without_replacing_passenger_identity() -> TestResult {
    let owner = vehicle_owner()?;
    let passenger = Rc::new(Cell::new(false));
    let mut random = CrtRand::new();
    owner.advance_scene(100., &mut random)?;
    owner.start_vehicle_ride_animation(7, &passenger, -1, 115, 110, &mut random)?;
    assert!(owner.vehicle_controls_body_key(-1));
    assert!(!owner.vehicle_controls_body_key(26));
    owner.select(
        &mut owner.playback.borrow_mut(),
        91.into(),
        input(0),
        120,
        M2SequenceStartPhase::BeforeSceneUpdate,
        &mut random,
    )?;
    assert!(!owner.vehicle_controls_body_key(-1));
    assert_eq!(owner.playback.borrow().animation_id, 91);
    let before = random;
    owner.release_vehicle_ride_animation(7, -1, 130, &mut random)?;
    assert_eq!(random, before);
    assert_eq!(owner.playback.borrow().animation_id, 91);
    Ok(())
}

#[test]
fn retired_vehicle_passenger_releases_root_in_the_same_callback() -> TestResult {
    let owner = vehicle_owner()?;
    let passenger = Rc::new(Cell::new(false));
    let mut random = CrtRand::new();
    owner.advance_scene(100., &mut random)?;
    owner.start_vehicle_ride_animation(7, &passenger, -1, 115, 110, &mut random)?;
    passenger.set(true);
    owner.advance_scene(370., &mut random)?;
    assert!(!owner.vehicle_controls_body_key(-1));
    let playback = owner.playback.borrow();
    assert_eq!(playback.animation_id, 0);
    assert_eq!(
        playback.script_timer.ok_or("resumed root")?.start_time_ms(),
        370
    );
    Ok(())
}

#[test]
fn vehicle_registry_has_sixteen_slots_and_frees_every_record_for_a_departing_guid() -> TestResult {
    let owner = vehicle_owner()?;
    let passenger = Rc::new(Cell::new(false));
    let mut random = CrtRand::new();
    owner.advance_scene(100., &mut random)?;
    for guid in 1..=17 {
        owner.start_vehicle_ride_animation(
            guid,
            &passenger,
            4,
            115,
            100 + guid as u32,
            &mut random,
        )?;
    }
    // The seventeenth start succeeds but its owner record cannot be stored.
    for guid in 1..=16 {
        owner.release_vehicle_ride_animation(guid, 4, 130, &mut random)?;
    }
    assert!(!owner.vehicle_controls_body_key(4));
    assert!(
        owner
            .playback
            .borrow()
            .bone_playback(4)
            .ok_or("upper")?
            .script_timer
            .is_none()
    );
    owner.start_vehicle_ride_animation(7, &passenger, 4, 115, 140, &mut random)?;
    owner.start_vehicle_ride_animation(7, &passenger, 4, 115, 150, &mut random)?;
    owner.release_vehicle_ride_animation(7, 4, 160, &mut random)?;
    assert!(!owner.vehicle_controls_body_key(4));
    Ok(())
}
