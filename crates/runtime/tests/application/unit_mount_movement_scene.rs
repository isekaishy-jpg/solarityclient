//! All resident unit paths apply movement rates to their independent mount.

use super::*;
use solarity_ecs::{WorldMovementSpeeds, WorldMovementState};

#[test]
fn mount_jump_and_landing_callbacks_reach_all_offscreen_unit_paths() -> Result<(), Box<dyn Error>> {
    use crate::application::unit_animation::{
        UnitMovementAnimationEvent, UnitMovementAnimationEventKind,
    };
    let _sdl_guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let fixture = crate::test_support::unit_models::fixture_with_mount_scale()?;
    let mut presentation = unit_presentation(&fixture)?;
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        Vec3::ZERO,
        0.,
    ));
    for (guid, kind) in [
        (7, ObjectKind::Player),
        (20, ObjectKind::Player),
        (30, ObjectKind::Unit),
    ] {
        add_unit(&mut world, guid, kind, 0)?;
        equipment_residency::fields(&mut world, guid, &[(69, 102)])?;
    }
    presentation.synchronize(Some(&world))?;
    presentation.synchronize_remote_players(Some(&world))?;
    presentation.synchronize_creatures(Some(&world), |_| None)?;
    let platform = SdlPlatform::start(WindowConfiguration::new(128, 128, WindowMode::Windowed))?;
    let mut renderer = renderer(&platform)?;
    let mut random = CrtRand::new();
    let mut frame = M2Frame::prepare(
        &mut renderer,
        &ResidentM2Scene::default(),
        fixture_animations(&fixture)?,
        &mut random,
        Arc::new(M2ParticleTwinkleTable::new(1)),
    )?;
    frame.replace_player(
        &mut renderer,
        presentation.resident_frame_input(),
        &mut random,
    )?;
    frame.replace_remote_players(
        &mut renderer,
        &presentation.resident_remote_player_frame_inputs(),
        &mut random,
    )?;
    frame.replace_creatures(
        &mut renderer,
        &presentation.resident_creature_frame_inputs(),
        &mut random,
    )?;
    let camera = WorldCamera::stock(
        Vec3::new(100., 0., 0.),
        Vec3::new(200., 0., 0.),
        Vec3::Z,
        100.,
    )
    .frame(1.)?;
    for (step, (now, expected_animation)) in
        [(100., 37), (500., 37), (1500., 38), (1600., 39), (2800., 0)]
            .into_iter()
            .enumerate()
    {
        let before = random;
        if matches!(step, 0 | 3) {
            let movement = WorldMovementState::new(
                if step == 0 { 0x3000 } else { 0 },
                WorldMovementSpeeds::new([2.5, 7., 4.5, 4.72, 2.5, 7., 4.5, 3., 3.]),
                Default::default(),
            );
            for guid in [7, 20, 30] {
                world.update_movement(guid, movement)?;
                presentation.notify_movement_animation(UnitMovementAnimationEvent {
                    identity: world.object_identity(guid).ok_or("identity")?,
                    movement,
                    stand: 0,
                    kind: if step == 0 {
                        UnitMovementAnimationEventKind::Jump
                    } else {
                        UnitMovementAnimationEventKind::Land {
                            previous_flags: 0x3000,
                            forced: false,
                            slow: true,
                        }
                    },
                });
            }
            presentation.synchronize(Some(&world))?;
            presentation.synchronize_remote_players(Some(&world))?;
            presentation.synchronize_creatures(Some(&world), |_| None)?;
        }
        frame.update_player_state(
            presentation.resident_frame_input().ok_or("local")?,
            now,
            &mut random,
        )?;
        frame.update_remote_player_states(
            &presentation.resident_remote_player_frame_inputs(),
            now,
            &mut random,
        )?;
        frame.update_creature_states(
            &presentation.resident_creature_frame_inputs(),
            now,
            &mut random,
        )?;
        let draws = frame.prepare_visible_draws(
            &renderer,
            WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
            camera,
            solarity_rendering::M2TransparentPass::One,
            Vec3::ZERO,
            now,
            M2CameraEffectScale::EXTERNAL_CAMERA,
            &mut random,
            None,
        )?;
        assert!(
            draws.draws.is_empty(),
            "callbacks must execute before culling"
        );
        for (mount, body) in [
            (
                M2GpuPlacementOwner::PlayerMount { guid: 7 },
                M2GpuPlacementOwner::PlayerBody { guid: 7 },
            ),
            (
                M2GpuPlacementOwner::RemotePlayerMount { guid: 20 },
                M2GpuPlacementOwner::RemotePlayerBody { guid: 20 },
            ),
            (
                M2GpuPlacementOwner::CreatureMount { guid: 30 },
                M2GpuPlacementOwner::CreatureBody { guid: 30 },
            ),
        ] {
            for (owner, expected) in [(mount, expected_animation), (body, 91)] {
                let placement = frame
                    .placements
                    .iter()
                    .find(|p| p.owner == owner)
                    .ok_or("placement")?;
                assert_eq!(
                    placement
                        .playback
                        .as_ref()
                        .ok_or("clock")?
                        .borrow()
                        .animation_id,
                    expected,
                    "step {step}: {owner:?}"
                );
            }
        }
        let mut expected = before;
        if step != 1 {
            for _ in 0..6 {
                let _roll = expected.next_u15();
            }
        }
        assert_eq!(
            random, expected,
            "step {step}: one shared stream across three mounts"
        );
    }
    Ok(())
}

#[test]
fn mount_stride_rates_reach_local_remote_and_creature_scene_timers() -> Result<(), Box<dyn Error>> {
    let _sdl_guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let fixture = crate::test_support::unit_models::fixture_with_mount_scale()?;
    let mut presentation = unit_presentation(&fixture)?;
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        Vec3::ZERO,
        0.,
    ));
    for (guid, kind) in [
        (7, ObjectKind::Player),
        (20, ObjectKind::Player),
        (30, ObjectKind::Unit),
    ] {
        add_unit(&mut world, guid, kind, 0)?;
        equipment_residency::fields(&mut world, guid, &[(69, 102)])?;
    }
    let platform = SdlPlatform::start(WindowConfiguration::new(128, 128, WindowMode::Windowed))?;
    let mut renderer = renderer(&platform)?;
    let mut random = CrtRand::new();
    let mut frame = M2Frame::prepare(
        &mut renderer,
        &ResidentM2Scene::default(),
        fixture_animations(&fixture)?,
        &mut random,
        Arc::new(M2ParticleTwinkleTable::new(1)),
    )?;
    let owners = [
        M2GpuPlacementOwner::PlayerMount { guid: 7 },
        M2GpuPlacementOwner::RemotePlayerMount { guid: 20 },
        M2GpuPlacementOwner::CreatureMount { guid: 30 },
    ];
    let mut previous = Vec::new();
    for (step, speed) in [7_f32, 10.5, 10.5, 7.].into_iter().enumerate() {
        for guid in [7, 20, 30] {
            let speeds = [2.5, speed, 4.5, 4.72, 2.5, 7., 4.5, 3., 3.];
            world.update_movement(
                guid,
                WorldMovementState::new(1, WorldMovementSpeeds::new(speeds), Default::default()),
            )?;
        }
        presentation.synchronize(Some(&world))?;
        presentation.synchronize_remote_players(Some(&world))?;
        presentation.synchronize_creatures(Some(&world), |_| None)?;
        // Rebuilding the local body synchronizes its unit owner, including the
        // retained mount. Count the whole replacement/update transaction.
        let before_replace = random;
        frame.replace_player(
            &mut renderer,
            presentation.resident_frame_input(),
            &mut random,
        )?;
        frame.replace_remote_players(
            &mut renderer,
            &presentation.resident_remote_player_frame_inputs(),
            &mut random,
        )?;
        frame.replace_creatures(
            &mut renderer,
            &presentation.resident_creature_frame_inputs(),
            &mut random,
        )?;
        // Creation itself must receive the unit's speed. Later changes pass
        // through each update path without replacing the resident mount.
        let before_update = if step == 0 { random } else { before_replace };
        let now = 2000 + step as u32 * 100;
        if step != 0 {
            frame.update_player_state(
                presentation.resident_frame_input().ok_or("local")?,
                now as f32,
                &mut random,
            )?;
            frame.update_remote_player_states(
                &presentation.resident_remote_player_frame_inputs(),
                now as f32,
                &mut random,
            )?;
            frame.update_creature_states(
                &presentation.resident_creature_frame_inputs(),
                now as f32,
                &mut random,
            )?;
        }
        for (index, owner) in owners.iter().enumerate() {
            let placement = frame
                .placements
                .iter()
                .find(|p| p.owner == *owner)
                .ok_or("mount")?;
            let playback = placement.playback.as_ref().ok_or("playback")?.borrow();
            let timer = playback.script_timer.ok_or("mount timer")?;
            assert_eq!(playback.animation_id, 5, "{owner:?}");
            assert_eq!(timer.speed(), speed / 3.5, "{owner:?}");
            if step == 0 {
                previous.push((placement.source_index, timer));
            } else {
                assert_eq!(placement.source_index, previous[index].0, "{owner:?}");
                if step == 2 {
                    assert_eq!(timer, previous[index].1, "unchanged rate {owner:?}");
                } else {
                    assert_ne!(timer, previous[index].1, "changed rate {owner:?}");
                }
                previous[index].1 = timer;
            }
        }
        for owner in [
            M2GpuPlacementOwner::PlayerBody { guid: 7 },
            M2GpuPlacementOwner::RemotePlayerBody { guid: 20 },
            M2GpuPlacementOwner::CreatureBody { guid: 30 },
        ] {
            let body = frame
                .placements
                .iter()
                .find(|p| p.owner == owner)
                .ok_or("rider")?;
            let playback = body.playback.as_ref().ok_or("body playback")?.borrow();
            assert_eq!(playback.animation_id, 91);
            assert_eq!(playback.script_timer.ok_or("body timer")?.speed(), 1.);
        }
        let mut expected_random = before_update;
        if matches!(step, 1 | 3) {
            for _ in 0..6 {
                let _roll = expected_random.next_u15();
            }
        }
        assert_eq!(random, expected_random, "step {step}");
    }
    Ok(())
}
