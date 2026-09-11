//! Mounted requests share Unit_C ordering while retaining independent clocks.

use super::*;

#[path = "unit_mount_routing.rs"]
mod routing;

fn bind_mount(
    owner: &UnitAnimationBehavior,
    mount: &UnitAnimationBehavior,
    now: u32,
    random: &mut CrtRand,
) -> Result<Rc<RefCell<M2Playback>>, Box<dyn Error>> {
    let playback = Rc::new(RefCell::new(M2Playback::default_sequence(
        &mount.model,
        &mount.animations,
        now,
        random,
    )?));
    owner.bind_mount(&mount.model, Rc::clone(&playback));
    owner.synchronize(now, random)?;
    Ok(playback)
}

fn advance_mount(
    owner: &UnitAnimationBehavior,
    model: &DecodedM2Model,
    playback: &mut M2Playback,
    now: u32,
    random: &mut CrtRand,
) -> Result<(), RuntimeTerrainFrameError> {
    let mut completed =
        |playback: &mut M2Playback, key: i32, animation: u16, _: u32, random: &mut CrtRand| {
            owner.complete_mount_animation(model, playback, key, animation, random)
        };
    playback.clock_with_bone_callbacks(model, now, random, Some(&mut completed), None)?;
    Ok(())
}

#[test]
fn mount_jump_completion_and_landing_keep_the_riders_timer() -> Result<(), Box<dyn Error>> {
    let mut mounted = input(0);
    mounted.mounted = true;
    let owner = owner_with_input(POSES, mounted)?;
    let mount = owner_with_input(POSES, input(0))?;
    let mut random = CrtRand::new();
    let clock = bind_mount(&owner, &mount, 100, &mut random)?;
    let rider_timer = owner.playback.borrow().script_timer;
    notify(
        &owner,
        movement(0x1000, Some(-7.95555)),
        UnitMovementAnimationEventKind::Jump,
    );
    owner.synchronize(200, &mut random)?;
    assert_eq!(clock.borrow().animation_id, 37);
    let jump_timer = clock.borrow().script_timer.ok_or("jump timer")?;
    assert_eq!(jump_timer.start_time_ms(), 201);
    let before = random;
    owner.synchronize(300, &mut random)?;
    advance_mount(
        &owner,
        &mount.model,
        &mut clock.borrow_mut(),
        300,
        &mut random,
    )?;
    assert_eq!(clock.borrow().script_timer, Some(jump_timer));
    assert_eq!(random, before, "no new Unit request may overwrite takeoff");
    advance_mount(
        &owner,
        &mount.model,
        &mut clock.borrow_mut(),
        1500,
        &mut random,
    )?;
    assert_eq!(clock.borrow().animation_id, 38);
    assert_eq!(
        clock.borrow().script_timer.ok_or("loop")?.start_time_ms(),
        1500,
        "73BFF0 resumes at the scene tick without the body's overdue offset"
    );
    let mut expected = before;
    for _ in 0..2 {
        let _roll = expected.next_u15();
    }
    assert_eq!(random, expected);
    notify(
        &owner,
        movement(0, None),
        UnitMovementAnimationEventKind::Land {
            previous_flags: 0x3000,
            forced: false,
            slow: true,
        },
    );
    owner.synchronize(1600, &mut random)?;
    assert_eq!(clock.borrow().animation_id, 39);
    assert!(
        !owner.landing.get(),
        "mount submission does not set the body's landing bit"
    );
    advance_mount(
        &owner,
        &mount.model,
        &mut clock.borrow_mut(),
        2800,
        &mut random,
    )?;
    assert_eq!(clock.borrow().animation_id, 0);
    assert_eq!(owner.playback.borrow().animation_id, 91);
    assert_eq!(owner.playback.borrow().script_timer, rider_timer);
    Ok(())
}

#[test]
fn mount_airborne_reevaluation_reads_body_behavior_and_preserves_packet_order()
-> Result<(), Box<dyn Error>> {
    let mut mounted = input(0);
    mounted.mounted = true;
    let owner = owner_with_input(POSES, mounted)?;
    let mount = owner_with_input(POSES, input(0))?;
    let mut random = CrtRand::new();
    let clock = bind_mount(&owner, &mount, 100, &mut random)?;
    let before = random;
    notify(
        &owner,
        movement(0x1000, Some(-7.95555)),
        UnitMovementAnimationEventKind::Jump,
    );
    notify(
        &owner,
        movement(0x1001, Some(-7.95555)),
        UnitMovementAnimationEventKind::Changed,
    );
    owner.synchronize(200, &mut random)?;
    assert_eq!(
        clock.borrow().animation_id,
        40,
        "724200 tests body 91, so a later airborne request can interrupt mount 37"
    );
    let mut expected = before;
    for _ in 0..4 {
        let _roll = expected.next_u15();
    }
    assert_eq!(random, expected, "both captured requests execute in order");
    notify(
        &owner,
        movement(0x1000, Some(-7.95555)),
        UnitMovementAnimationEventKind::Jump,
    );
    notify(
        &owner,
        movement(0, None),
        UnitMovementAnimationEventKind::Land {
            previous_flags: 0x3000,
            forced: false,
            slow: true,
        },
    );
    owner.synchronize(300, &mut random)?;
    assert_eq!(clock.borrow().animation_id, 39);
    assert_eq!(owner.playback.borrow().animation_id, 91);
    for _ in 0..4 {
        let _roll = expected.next_u15();
    }
    assert_eq!(random, expected);
    Ok(())
}

#[test]
fn mount_terminal_completion_can_reissue_the_same_animation() -> Result<(), Box<dyn Error>> {
    let mut mounted = input(0).with_movement(movement(0x3000, None));
    mounted.mounted = true;
    let owner = owner_with_input(POSES, mounted)?;
    let mount = owner_with_sequence_metadata(POSES, input(0), |_, id, sequence| {
        if id == 40 {
            sequence[12..16].copy_from_slice(&0x21_u32.to_le_bytes());
        }
    })?;
    let mut random = CrtRand::new();
    let clock = bind_mount(&owner, &mount, 100, &mut random)?;
    assert_eq!(clock.borrow().animation_id, 40);
    let before = random;
    advance_mount(
        &owner,
        &mount.model,
        &mut clock.borrow_mut(),
        1500,
        &mut random,
    )?;
    assert_eq!(clock.borrow().animation_id, 40);
    assert_eq!(
        clock
            .borrow()
            .script_timer
            .ok_or("reissued timer")?
            .start_time_ms(),
        1500
    );
    let mut expected = before;
    for _ in 0..2 {
        let _roll = expected.next_u15();
    }
    assert_eq!(random, expected);
    Ok(())
}

#[test]
fn mount_pending_jump_and_clock_survive_body_model_replacement() -> Result<(), Box<dyn Error>> {
    let mut mounted = input(0);
    mounted.mounted = true;
    let original = owner_with_input(POSES, mounted)?;
    let fixture = ClientFixture::with_common_files(&[
        (
            "Solarity\\Replacement.m2",
            &models::model_with_animations(&[0, 91])?,
        ),
        ("Solarity\\Replacement00.skin", &models::skin()?),
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let replacement = Arc::new(DecodedM2Model::load(
        &mut store,
        &AssetPath::new("Solarity\\Replacement.m2")?,
    )?);
    let mut scene = UnitAnimationScene::default();
    scene.bind(
        original.identity,
        &original.model,
        &original.animations,
        mounted,
    );
    let first = Rc::clone(scene.get(original.identity.guid()).ok_or("first body")?);
    let mut random = CrtRand::new();
    let clock = bind_mount(&first, &original, 100, &mut random)?;
    notify(
        &first,
        movement(0x1000, Some(-7.95555)),
        UnitMovementAnimationEventKind::Jump,
    );
    scene.bind(
        original.identity,
        &replacement,
        &original.animations,
        first.input.get(),
    );
    let second = scene
        .get(original.identity.guid())
        .ok_or("replacement body")?;
    assert!(!Rc::ptr_eq(&first, second));
    assert!(Rc::ptr_eq(
        &clock,
        &second
            .mount_model
            .borrow()
            .as_ref()
            .ok_or("retained mount")?
            .playback
    ));
    assert!(first.pending.borrow().is_empty());
    second.synchronize(200, &mut random)?;
    assert_eq!(
        clock.borrow().animation_id,
        37,
        "captured Jump must survive the body resource change"
    );
    assert_eq!(second.playback.borrow().animation_id, 91);
    advance_mount(
        second,
        &original.model,
        &mut clock.borrow_mut(),
        1500,
        &mut random,
    )?;
    assert_eq!(clock.borrow().animation_id, 38);
    Ok(())
}

#[test]
fn mount_request_before_dismount_still_consumes_its_ordered_selection() -> Result<(), Box<dyn Error>>
{
    let mut mounted = input(0);
    mounted.mounted = true;
    let owner = owner_with_input(POSES, mounted)?;
    let mount = owner_with_input(POSES, input(0))?;
    let mut random = CrtRand::new();
    let clock = bind_mount(&owner, &mount, 100, &mut random)?;
    let before = random;
    notify(
        &owner,
        movement(0x1000, Some(-7.95555)),
        UnitMovementAnimationEventKind::Jump,
    );
    owner.set_input(input(0));
    owner.synchronize(200, &mut random)?;
    assert_eq!(clock.borrow().animation_id, 37);
    assert_eq!(owner.playback.borrow().animation_id, 0);
    assert!(owner.mount_model.borrow().is_none());
    let mut expected = before;
    for _ in 0..4 {
        let _roll = expected.next_u15();
    }
    assert_eq!(random, expected, "mount Jump precedes unmounted body Stand");
    Ok(())
}
