//! Native seat joins and the live model admission consumer.

use super::*;
use crate::application::terrain_frame::m2::M2TransparentDrawIndex;
use solarity_asset::VehicleCatalog;
use solarity_ecs::{
    WorldMovementContext, WorldMovementSpeeds, WorldMovementState, WorldMovementTransport,
};
use solarity_rendering::m2_model_distance_key;

#[path = "vehicle_owned_scene.rs"]
mod vehicle_owned_scene;

#[test]
fn unloaded_passenger_uses_current_parent_bones_and_keeps_travel_when_its_model_arrives()
-> Result<(), Box<dyn Error>> {
    use crate::application::unit_animation::{
        UnitMovementAnimationEvent, UnitMovementAnimationEventKind,
    };
    use solarity_systems::{
        VehiclePassengerPhase as Phase, VehiclePassengerTransition, VehicleSeatPose,
        VehicleTransitionInput, vehicle_entry_target,
    };

    let _sdl_guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let parameters = [0.25, 4., 20., 0., 10., 0., 20.];
    let fixture = crate::test_support::unit_models::fixture_with_vehicle_entry(parameters)?;
    let platform = SdlPlatform::start(WindowConfiguration::new(128, 128, WindowMode::Windowed))?;
    let mut renderer = renderer(&platform)?;
    let camera = WorldCamera::orthographic(
        Vec3::new(30., 0., 10.),
        Vec3::ZERO,
        Vec3::Z,
        [-30., 30.],
        [-30., 30.],
        0.1,
        100.,
    )
    .frame(1.)?;
    for (mounted, parent_late) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut presentation = unit_presentation(&fixture)?;
        let mut world = ActiveWorld::enter(WorldBootstrap::new(
            WorldMapId::new(0),
            7,
            "Local",
            Vec3::ZERO,
            0.,
        ));
        for guid in [10, 30] {
            world.create_object(
                guid,
                ObjectKind::Unit,
                Some(WorldTransform::new(Vec3::ZERO, 0.)),
                [(4, 1_f32.to_bits())],
            )?;
        }
        world.set_unit_vehicle(30, 1, 0.);
        world.update_transform(10, WorldTransform::new(Vec3::X * 2., 0.))?;
        let ready = |world: &mut ActiveWorld, guid, mount| {
            equipment_residency::fields(
                world,
                guid,
                &[
                    (4, 1_f32.to_bits()),
                    (23, u32::from_le_bytes([1, 1, 0, 0])),
                    (24, 100),
                    (32, 100),
                    (67, 100),
                    (68, 100),
                    (69, if mount { 102 } else { 0 }),
                    (74, 0),
                ],
            )
        };
        if !parent_late {
            ready(&mut world, 30, mounted)?;
        }
        let child = world.object_identity(10).ok_or("child")?;
        let parent = world.object_identity(30).ok_or("parent")?;
        let mut random = CrtRand::new();
        let mut frame = M2Frame::prepare(
            &mut renderer,
            &ResidentM2Scene::default(),
            fixture_animations(&fixture)?,
            &mut random,
            Arc::new(M2ParticleTwinkleTable::new(1)),
        )?;
        for now in [0_u32, 100, 350] {
            presentation.set_animation_scene_time(now);
            if now == 100 {
                let movement = WorldMovementState::new(
                    0x200,
                    WorldMovementSpeeds::new([0.; 9]),
                    WorldMovementContext {
                        transport: Some(WorldMovementTransport {
                            guid: 30,
                            position: Vec3::X * 2.,
                            orientation: 0.,
                            time_ms: now,
                            seat: 2,
                            interpolated_time_ms: None,
                        }),
                        ..Default::default()
                    },
                );
                world.update_movement(10, movement)?;
                presentation.notify_movement_animation(UnitMovementAnimationEvent {
                    identity: child,
                    movement,
                    stand: 0,
                    kind: UnitMovementAnimationEventKind::Passenger {
                        previous_transform: WorldTransform::new(Vec3::X * 10., 0.),
                        previous: None,
                        parent: Some(parent),
                        animated: true,
                    },
                });
            }
            if now == 350 && parent_late {
                ready(&mut world, 30, mounted)?;
            }
            presentation.synchronize(Some(&world))?;
            presentation.synchronize_creatures(Some(&world), |_| None)?;
            frame.replace_creatures(
                &mut renderer,
                &presentation.resident_creature_frame_inputs(),
                &mut random,
            )?;
            let before = random;
            frame.advance_unbound_passengers(
                presentation.movement_animations(),
                now as f32,
                &mut random,
            )?;
            assert_eq!(
                random, before,
                "seat lookup must not advance the parent's animation"
            );
            assert!(presentation.movement_animations().get(10).is_none());
            assert!(
                !frame
                    .placements
                    .iter()
                    .any(|p| p.owner == M2GpuPlacementOwner::CreatureBody { guid: 10 })
            );
            if now < 350 {
                frame.update_creature_states(
                    &presentation.resident_creature_frame_inputs(),
                    now as f32,
                    &mut random,
                )?;
                equipment_residency::advance(
                    &mut frame,
                    &renderer,
                    camera,
                    now as f32,
                    &mut random,
                )?;
            }
        }
        let mut controllers = Vec::new();
        presentation
            .movement_animations()
            .collect_passenger_transitions(&mut controllers);
        let controller = controllers
            .into_iter()
            .find(|c| c.identity() == child)
            .ok_or("unloaded child transition")?;
        let request = controller.target().ok_or("entry request")?;
        assert!(request.anchor.is_none());
        let parent_kind = if mounted {
            M2GpuPlacementOwner::CreatureMount { guid: 30 }
        } else {
            M2GpuPlacementOwner::CreatureBody { guid: 30 }
        };
        // The authored bone moves +2 Z and scales 1 -> 2 over a one-second clip.
        // Compute it from the retained selected timer, independently of the seat sampler.
        let target_at = |frame: &M2Frame, now, anchor| -> Result<Vec3, Box<dyn Error>> {
            let placement = frame
                .placements
                .iter()
                .find(|p| p.owner == parent_kind)
                .ok_or("resident vehicle model")?;
            let playback = placement.playback.as_ref().ok_or("playback")?.borrow();
            let phase = playback.script_timer.map_or_else(
                || {
                    (now as f32 - playback.cycle_started_ms)
                        .rem_euclid(playback.sequence_duration_ms)
                },
                |timer| timer.animation_time_ms(now) as f32,
            ) / 1000.;
            let bone = Mat4::from_translation(Vec3::Z * (2. * phase))
                * Mat4::from_scale(Vec3::splat(1. + phase));
            let seat = request.parent.seat.ok_or("seat")?;
            Ok(vehicle_entry_target(
                VehicleSeatPose {
                    passenger_yaw: 0.,
                    rotation: Vec3::from_array(seat.passenger_rotation()),
                    offset: Vec3::from_array(seat.attachment_offset()),
                    passenger_anchor: anchor,
                    passenger_scale: 1.,
                    vehicle_scale: 1.,
                    attachment: Some(
                        placement.transform * bone * Mat4::from_translation(Vec3::new(2., -1., 3.)),
                    ),
                    vehicle_position: request.parent.parent_pose.position(),
                    vehicle_yaw: request.parent.parent_pose.orientation(),
                },
                placement.transform,
                request.parent.parent_frame,
                false,
            ))
        };
        let target = target_at(&frame, 350, None)?;
        let mut expected = VehiclePassengerTransition::new(VehicleTransitionInput {
            phase: Phase::Entering,
            has_parent: true,
            parameters,
            origin: Vec3::X * 2.,
            target,
            unit_position: Vec3::X * 2.,
            parent_velocity: Vec3::ZERO,
            yaw: 0.,
            previous_yaw: 0.,
            start_ms: 350,
        });
        let end = expected.end_ms();
        assert!(end > 450);
        assert!(
            !controller.needs_advance(end - 1),
            "duration must reach the animated seat ({mounted}, {parent_late}): {end}"
        );
        assert!(
            controller.needs_advance(end),
            "timer must use current bones at phase entry"
        );
        let arrival = 350 + (end - 350) / 2;
        ready(&mut world, 10, false)?;
        presentation.set_animation_scene_time(arrival);
        presentation.synchronize(Some(&world))?;
        presentation.synchronize_creatures(Some(&world), |_| None)?;
        frame.replace_creatures(
            &mut renderer,
            &presentation.resident_creature_frame_inputs(),
            &mut random,
        )?;
        frame.advance_unbound_passengers(
            presentation.movement_animations(),
            arrival as f32,
            &mut random,
        )?;
        frame.update_creature_states(
            &presentation.resident_creature_frame_inputs(),
            arrival as f32,
            &mut random,
        )?;
        equipment_residency::advance(&mut frame, &renderer, camera, arrival as f32, &mut random)?;
        let placement = frame
            .placements
            .iter()
            .find(|p| p.owner == M2GpuPlacementOwner::CreatureBody { guid: 10 })
            .ok_or("arrived child")?;
        let owner = placement.unit_animation.as_ref().ok_or("arrived owner")?;
        assert_eq!(owner.passenger_phase(), Phase::Entering);
        let anchor = owner.passenger_anchor(
            &frame.sources[placement.source_index]
                .as_ref()
                .ok_or("source")?
                .model,
        );
        assert_eq!(
            anchor,
            Some(Vec3::new(0.25, 0.5, 1.)),
            "absent model must not cache an absent attachment"
        );
        let target = target_at(&frame, arrival, anchor)?;
        let expected_position = expected
            .sample(
                arrival,
                request.parent.seat.ok_or("seat")?.flags(),
                Vec3::X * 2.,
                target,
                0.,
            )
            .position;
        assert!(
            placement
                .transform
                .w_axis
                .truncate()
                .abs_diff_eq(expected_position, 0.0001),
            "arrival continues its existing travel: {:?} != {expected_position:?}",
            placement.transform.w_axis.truncate()
        );
        assert!(!controller.needs_advance(end - 1));
        assert!(controller.needs_advance(end));
    }
    Ok(())
}

#[test]
fn vehicle_passenger_transition_changes_render_parent_only_when_the_model_attaches()
-> Result<(), Box<dyn Error>> {
    use crate::application::unit_animation::{
        UnitMovementAnimationEvent, UnitMovementAnimationEventKind,
    };
    use solarity_systems::VehiclePassengerPhase as Phase;

    let _sdl_guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let fixture = crate::test_support::unit_models::fixture_with_vehicle_seats()?;
    let mut presentation = unit_presentation(&fixture)?;
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        Vec3::ZERO,
        0.,
    ));
    for guid in [10, 30] {
        add_unit(&mut world, guid, ObjectKind::Unit, 0)?;
    }
    world.set_unit_vehicle(30, 1, 0.);
    world.update_transform(10, WorldTransform::new(Vec3::X * 10., 0.))?;
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
    let camera = WorldCamera::orthographic(
        Vec3::new(30., 0., 10.),
        Vec3::ZERO,
        Vec3::Z,
        [-30., 30.],
        [-30., 30.],
        0.1,
        100.,
    )
    .frame(1.)?;
    let child_identity = world.object_identity(10).ok_or("child identity")?;
    let parent_identity = world.object_identity(30).ok_or("parent identity")?;
    let mut entry_origin = None;
    let mut exit_origin = None;
    for (now, phase, attached) in [
        (0, Phase::Detached, false),
        (100, Phase::EnterDelay, false),
        (350, Phase::Entering, false),
        (1350, Phase::Entering, false),
        (2350, Phase::Seated, true),
        (2400, Phase::ExitDelay, true),
        (2525, Phase::Exiting, false),
        (2775, Phase::Exiting, false),
        (3025, Phase::Detached, false),
    ] {
        presentation.set_animation_scene_time(now);
        if now == 100 || now == 2400 {
            let previous_transform = world.object_transform(10).ok_or("previous transform")?;
            let previous = world
                .movement_state(10)
                .and_then(|movement| movement.context().transport)
                .map(|transport| (transport.guid, transport.seat));
            let parent = (now == 100).then_some(parent_identity);
            let movement = WorldMovementState::new(
                if parent.is_some() { 0x200 } else { 0 },
                WorldMovementSpeeds::new([0.; 9]),
                WorldMovementContext {
                    transport: parent.map(|parent| WorldMovementTransport {
                        guid: parent.guid(),
                        position: Vec3::X * 2.,
                        orientation: 0.,
                        time_ms: now,
                        seat: 2,
                        interpolated_time_ms: None,
                    }),
                    ..Default::default()
                },
            );
            world.update_movement(10, movement)?;
            world.update_transform(
                10,
                WorldTransform::new(Vec3::X * if parent.is_some() { 2. } else { 6. }, 0.),
            )?;
            presentation.notify_movement_animation(UnitMovementAnimationEvent {
                identity: child_identity,
                movement,
                stand: 0,
                kind: UnitMovementAnimationEventKind::Passenger {
                    previous_transform,
                    previous,
                    parent,
                    animated: true,
                },
            });
        }
        presentation.synchronize_creatures(Some(&world), |_| None)?;
        frame.replace_creatures(
            &mut renderer,
            &presentation.resident_creature_frame_inputs(),
            &mut random,
        )?;
        frame.update_creature_states(
            &presentation.resident_creature_frame_inputs(),
            now as f32,
            &mut random,
        )?;
        equipment_residency::advance(&mut frame, &renderer, camera, now as f32, &mut random)?;
        let child = frame
            .placements
            .iter()
            .position(|placement| placement.owner == M2GpuPlacementOwner::CreatureBody { guid: 10 })
            .ok_or("child")?;
        let parent = frame
            .placements
            .iter()
            .position(|placement| placement.owner == M2GpuPlacementOwner::CreatureBody { guid: 30 })
            .ok_or("parent")?;
        let owner = frame.placements[child]
            .unit_animation
            .as_ref()
            .ok_or("child animation")?;
        assert_eq!(owner.passenger_phase(), phase, "phase at {now}");
        assert_eq!(
            frame.placement_visibility.light_parent(child),
            attached.then_some(parent),
            "light parent at {now}"
        );
        assert_eq!(
            frame.placement_visibility.light_root(child),
            Some(if attached { parent } else { child }),
            "light root at {now}"
        );
        assert_eq!(
            frame.placements[child].last_effect_time_ms, now,
            "effect clock at {now}"
        );
        let position = frame.placements[child].transform.w_axis.truncate();
        match now {
            0 => entry_origin = Some(position),
            100 | 350 => assert_eq!(
                Some(position),
                entry_origin,
                "entry starts at previous rendered pose"
            ),
            1350 => {
                assert!(position.z > 5., "entry arc: {position:?}");
                assert_ne!(
                    position,
                    world.object_transform(10).ok_or("unit pose")?.position()
                );
            }
            2400 => exit_origin = Some(position),
            2525 => assert_eq!(
                Some(position),
                exit_origin,
                "exit starts at delayed seat pose"
            ),
            2775 => assert_ne!(Some(position), exit_origin),
            3025 => assert_eq!(position, Vec3::X * 6.),
            _ => {}
        }
    }
    Ok(())
}

#[test]
fn vehicle_seats_follow_animated_nested_models_without_rescaling_passengers()
-> Result<(), Box<dyn Error>> {
    nested_vehicle_scene(false)
}

#[test]
fn vehicle_seats_use_mount_models_and_preserve_attached_riders() -> Result<(), Box<dyn Error>> {
    nested_vehicle_scene(true)
}

fn nested_vehicle_scene(mounted: bool) -> Result<(), Box<dyn Error>> {
    let _sdl_guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let fixture = crate::test_support::unit_models::fixture_with_vehicle_seats()?;
    let model_owner = |guid| {
        if mounted {
            M2GpuPlacementOwner::CreatureMount { guid }
        } else {
            M2GpuPlacementOwner::CreatureBody { guid }
        }
    };
    let mut presentation = unit_presentation(&fixture)?;
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        Vec3::ZERO,
        0.,
    ));
    for (guid, scale) in [(10, 0.75_f32), (20, 1.25), (30, 2.)] {
        add_unit(&mut world, guid, ObjectKind::Unit, 0)?;
        equipment_residency::fields(&mut world, guid, &[(4, scale.to_bits())])?;
        if mounted {
            equipment_residency::fields(&mut world, guid, &[(69, 102)])?;
        }
        world.set_unit_vehicle(guid, 1, 0.);
    }
    world.update_transform(30, WorldTransform::new(Vec3::new(2., 1., 0.), 0.4))?;
    let board = |world: &mut ActiveWorld, child, parent, seat| -> Result<(), Box<dyn Error>> {
        world.update_movement(
            child,
            WorldMovementState::new(
                0x200,
                WorldMovementSpeeds::new([0.; 9]),
                WorldMovementContext {
                    transport: Some(WorldMovementTransport {
                        guid: parent,
                        position: Vec3::ZERO,
                        orientation: 0.,
                        time_ms: 0,
                        seat,
                        interpolated_time_ms: None,
                    }),
                    ..Default::default()
                },
            ),
        )?;
        Ok(())
    };
    board(&mut world, 10, 20, 2)?;
    board(&mut world, 20, 30, 2)?;
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
    let camera = WorldCamera::orthographic(
        Vec3::new(30., 0., 10.),
        Vec3::ZERO,
        Vec3::Z,
        [-30., 30.],
        [-30., 30.],
        0.1,
        100.,
    )
    .frame(1.)?;
    let base = solarity_rendering::M2SceneUniform::new(
        camera.projection(),
        camera.view(),
        camera.camera().position(),
        Vec3::ZERO,
        Vec3::ZERO,
        Vec3::Z,
        glam::Vec4::ZERO,
        Vec3::ZERO,
        [solarity_rendering::M2LocalLightState::disabled(); 4],
    );
    let exterior = solarity_rendering::M2DirectionalLight::new(-Vec3::Z, Vec3::ONE, Vec3::ONE);
    let mut previous_anchor = None;
    for now in [100_u32, 300, 700] {
        presentation.set_animation_scene_time(now);
        presentation.synchronize_creatures(Some(&world), |_| None)?;
        frame.replace_creatures(
            &mut renderer,
            &presentation.resident_creature_frame_inputs(),
            &mut random,
        )?;
        frame.update_creature_states(
            &presentation.resident_creature_frame_inputs(),
            now as f32,
            &mut random,
        )?;
        for placement in &mut frame.placements {
            placement.opacity = 0.8;
        }
        let projection = solarity_rendering::WorldShadowProjection::primary(
            solarity_rendering::WorldShadowQuality::UnitsHigh,
            if now == 300 {
                Vec3::X * 200.
            } else {
                Vec3::ZERO
            },
            camera.camera().position(),
            -Vec3::Z,
        )?;
        let visible = frame.prepare_visible_draws_with_unit_effects(
            &renderer,
            WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
            camera,
            solarity_rendering::M2TransparentPass::One,
            Vec3::ZERO,
            now as f32,
            M2CameraEffectScale::EXTERNAL_CAMERA,
            &mut random,
            None,
            None,
            Some((base, exterior)),
            None,
            Some(projection),
        )?;
        assert_eq!(visible.instance_scenes.len(), if mounted { 6 } else { 3 });
        for scene in visible.instance_scenes {
            assert_eq!(*scene, visible.instance_scenes[0]);
        }
        for (child_guid, parent_guid, scale) in [(10, 20, 0.75), (20, 30, 1.25)] {
            let child_index = frame
                .placements
                .iter()
                .position(|p| p.owner == model_owner(child_guid))
                .ok_or("child")?;
            let parent_index = frame
                .placements
                .iter()
                .position(|p| p.owner == model_owner(parent_guid))
                .ok_or("parent")?;
            assert!(
                child_index < parent_index,
                "test requires later parent placement"
            );
            assert_eq!(
                frame.placement_visibility.light_parent(child_index),
                Some(parent_index)
            );
            let root = frame
                .placements
                .iter()
                .position(|p| p.owner == model_owner(30))
                .ok_or("root")?;
            assert_eq!(
                frame.placement_visibility.light_root(child_index),
                Some(root)
            );
            assert!(frame.placement_visibility.model_distance_sort(child_index));
            let root_distance =
                m2_model_distance_key(camera.view() * frame.placements[root].transform);
            let mut mesh_count = 0;
            for element in &frame.transparent_elements {
                assert_eq!(
                    element.key.primary_distance(),
                    root_distance,
                    "attached meshes and effects share the root model's distance"
                );
                if matches!(element.draw, M2TransparentDrawIndex::Mesh(_)) {
                    mesh_count += 1;
                }
            }
            assert!(
                mesh_count >= 2,
                "translucent passengers exercise the sort keys"
            );
            assert_eq!(
                frame.shadow_admission[child_index],
                frame.shadow_admission[root]
            );
            assert_eq!(
                frame.shadow_admission[root],
                now != 300,
                "the entire attached hierarchy follows the root shadow volume"
            );
            let child = &frame.placements[child_index];
            let parent = &frame.placements[parent_index];
            let playback = parent.playback.as_ref().ok_or("playback")?.borrow();
            let phase = playback.script_timer.map_or_else(
                || {
                    (now as f32 - playback.cycle_started_ms)
                        .rem_euclid(playback.sequence_duration_ms)
                },
                |timer| timer.animation_time_ms(now) as f32,
            ) / 1000.;
            let bone = Mat4::from_translation(Vec3::new(0., 0., 2. * phase))
                * Mat4::from_scale(Vec3::splat(1. + phase));
            let expected_anchor = (parent.transform * bone)
                .transform_point3(Vec3::new(2., -1., 3.) + Vec3::new(0.5, 0.25, -0.5));
            let actual_anchor = child.transform.transform_point3(Vec3::new(0.25, 0.5, 1.));
            assert!(
                actual_anchor.abs_diff_eq(expected_anchor, 2e-5),
                "seat anchor {child_guid} at {now}: {actual_anchor:?} != {expected_anchor:?}"
            );
            for axis in [
                child.transform.x_axis,
                child.transform.y_axis,
                child.transform.z_axis,
            ] {
                assert!(
                    (axis.truncate().length() - scale).abs() < 2e-6,
                    "animated bone scale must cancel for passenger {child_guid}"
                );
            }
            assert_eq!(
                child.last_effect_time_ms, now,
                "effects consume the same frame"
            );
            if mounted {
                let rider = frame
                    .placements
                    .iter()
                    .find(|p| p.owner == M2GpuPlacementOwner::CreatureBody { guid: child_guid })
                    .ok_or("rider")?;
                let playback = child.playback.as_ref().ok_or("mount playback")?.borrow();
                let phase = playback.script_timer.map_or_else(
                    || {
                        (now as f32 - playback.cycle_started_ms)
                            .rem_euclid(playback.sequence_duration_ms)
                    },
                    |timer| timer.animation_time_ms(now) as f32,
                ) / 1000.;
                let bone = Mat4::from_translation(Vec3::new(0., 0., phase * 2.))
                    * Mat4::from_scale(Vec3::splat(1. + phase));
                let expected = child.transform
                    * bone
                    * Mat4::from_translation(Vec3::new(0.25, 0.5, 1.))
                    * Mat4::from_scale(Vec3::splat(rider.rider_scale));
                assert!(
                    rider.transform.abs_diff_eq(expected, 2e-5),
                    "the body follows the passenger mount's own animated saddle"
                );
                assert_eq!(rider.last_effect_time_ms, now);
            }
            if child_guid == 10 {
                if let Some(previous) = previous_anchor {
                    assert_ne!(actual_anchor, previous);
                }
                previous_anchor = Some(actual_anchor);
            }
        }
    }
    if mounted {
        opacity(&presentation, 30)?.set_player_hidden(true);
        equipment_residency::advance(&mut frame, &renderer, camera, 800., &mut random)?;
        assert!(
            frame.visible_draws.is_empty(),
            "hidden vehicle root suppresses every nested mount and body"
        );
        assert!(
            frame
                .placements
                .iter()
                .all(|p| p.last_effect_time_ms == 700)
        );
        return Ok(());
    }
    let child_pose = frame
        .placements
        .iter()
        .find(|p| p.owner == M2GpuPlacementOwner::CreatureBody { guid: 10 })
        .ok_or("child")?
        .transform;
    // A retired parent identity must not be rebound by passive GUID reuse.
    world.remove_object(20)?;
    add_unit(&mut world, 20, ObjectKind::Unit, 0)?;
    world.set_unit_vehicle(20, 1, 0.);
    world.update_transform(20, WorldTransform::new(Vec3::new(100., 0., 0.), 0.))?;
    presentation.synchronize_creatures(Some(&world), |_| None)?;
    frame.replace_creatures(
        &mut renderer,
        &presentation.resident_creature_frame_inputs(),
        &mut random,
    )?;
    frame.update_creature_states(
        &presentation.resident_creature_frame_inputs(),
        800.,
        &mut random,
    )?;
    equipment_residency::advance(&mut frame, &renderer, camera, 800., &mut random)?;
    assert_eq!(
        frame
            .placements
            .iter()
            .find(|p| p.owner == M2GpuPlacementOwner::CreatureBody { guid: 10 })
            .ok_or("child")?
            .transform,
        child_pose
    );
    presentation.passenger_frames.admit_parent(
        world.object_identity(10).ok_or("child identity")?,
        world.object_identity(20).ok_or("new parent identity")?,
    );
    presentation.synchronize_creatures(Some(&world), |_| None)?;
    frame.update_creature_states(
        &presentation.resident_creature_frame_inputs(),
        850.,
        &mut random,
    )?;
    equipment_residency::advance(&mut frame, &renderer, camera, 850., &mut random)?;
    assert_ne!(
        frame
            .placements
            .iter()
            .find(|p| p.owner == M2GpuPlacementOwner::CreatureBody { guid: 10 })
            .ok_or("child")?
            .transform,
        child_pose,
        "an explicit movement admission can reattach to the new generation"
    );
    // Missing seat rows take the upright world pose and clear the frozen link pose.
    board(&mut world, 10, 30, 1)?;
    presentation.synchronize_creatures(Some(&world), |_| None)?;
    frame.update_creature_states(
        &presentation.resident_creature_frame_inputs(),
        900.,
        &mut random,
    )?;
    equipment_residency::advance(&mut frame, &renderer, camera, 900., &mut random)?;
    let child = frame
        .placements
        .iter()
        .find(|p| p.owner == M2GpuPlacementOwner::CreatureBody { guid: 10 })
        .ok_or("child")?;
    assert_eq!(child.transform, child.local_transform);
    Ok(())
}

#[test]
fn vehicle_seat_join_and_fade_match_native_for_every_seat_byte() -> Result<(), Box<dyn Error>> {
    let fixture = crate::test_support::unit_models::fixture()?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let catalog = VehicleCatalog::load(&mut store)?;
    let mut count = 0;
    for line in include_str!("../fixtures/vehicle_seat_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let values = line
            .split_whitespace()
            .map(|word| u32::from_str_radix(word, 16))
            .collect::<Result<Vec<_>, _>>()?;
        let seat = if values[0] == 0 {
            None
        } else {
            catalog.passenger_seat(if values[0] == 2 { 1 } else { 0 }, values[1] as u8 as i8)
        };
        assert_eq!(seat.map_or(0, |seat| seat.id()), values[4], "{line}");
        let duration = solarity_systems::EntityOpacity::unit_entry_duration(
            0,
            0,
            0,
            0xf050_0000_0000_0001,
            (values[2] != 0).then_some(values[3] != 0),
            seat.map(|seat| seat.attachment_id()),
        );
        assert_eq!(duration, values[5] * 1000, "{line}");
        count += 1;
    }
    assert_eq!(count, 3072);
    Ok(())
}

#[test]
fn vehicle_passenger_model_admission_uses_attachment_and_keeps_selected_duration()
-> Result<(), Box<dyn Error>> {
    let fixture = crate::test_support::unit_models::fixture()?;
    let mut presentation = unit_presentation(&fixture)?;
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Passenger",
        Vec3::ZERO,
        0.0,
    ));
    add_unit(&mut world, 30, ObjectKind::Unit, 0)?;
    world.set_unit_vehicle(30, 1, 0.25);
    presentation.set_animation_scene_time(100);
    presentation.synchronize_creatures(Some(&world), |_| None)?;
    let parent = opacity(&presentation, 30)?;
    assert!(parent.transitioning());
    for (guid, seat) in [(31, 0), (32, 2), (33, 1), (34, -1)] {
        add_unit(&mut world, guid, ObjectKind::Unit, 0)?;
        world.update_movement(
            guid,
            WorldMovementState::new(
                0x200,
                WorldMovementSpeeds::new([0.; 9]),
                WorldMovementContext {
                    transport: Some(WorldMovementTransport {
                        guid: 30,
                        position: Vec3::ZERO,
                        orientation: 0.,
                        time_ms: 0,
                        seat,
                        interpolated_time_ms: None,
                    }),
                    ..Default::default()
                },
            ),
        )?;
    }
    presentation.synchronize_creatures(Some(&world), |_| None)?;
    let unattached = opacity(&presentation, 31)?;
    assert!(
        unattached.transitioning(),
        "negative attachment may fade with its parent"
    );
    assert_eq!(unattached.opacity(), 0.0);
    for guid in [32, 33, 34] {
        let passenger = opacity(&presentation, guid)?;
        assert!(
            !passenger.transitioning(),
            "bone-bound or unresolved seat publishes immediately"
        );
        assert_eq!(passenger.opacity(), 1.0);
    }
    unattached.advance(600);
    assert!((unattached.opacity() - 127.0 / 255.0).abs() < 1e-6);
    world.set_unit_vehicle(30, 0, 1.0);
    presentation.set_animation_scene_time(600);
    presentation.synchronize_creatures(Some(&world), |_| None)?;
    assert!(Rc::ptr_eq(&unattached, &opacity(&presentation, 31)?));
    assert!(
        unattached.transitioning(),
        "later seat changes cannot retime an admitted display"
    );
    unattached.advance(1100);
    assert_eq!(unattached.opacity(), 1.0);
    // A new identity must re-evaluate against the now unresolved vehicle row.
    world.remove_object(31)?;
    presentation.synchronize_creatures(Some(&world), |_| None)?;
    add_unit(&mut world, 31, ObjectKind::Unit, 0)?;
    world.update_movement(31, world.movement_state(32).ok_or("passenger movement")?)?;
    presentation.synchronize_creatures(Some(&world), |_| None)?;
    let replacement = opacity(&presentation, 31)?;
    assert!(!Rc::ptr_eq(&replacement, &unattached));
    assert_eq!(replacement.opacity(), 1.0);
    Ok(())
}

fn opacity(
    presentation: &crate::application::RuntimePlayerPresentation,
    guid: u64,
) -> Result<Rc<crate::application::entity_opacity::EntityOpacityOwner>, Box<dyn Error>> {
    presentation
        .resident_creature_frame_inputs()
        .iter()
        .find(|input| input.guid() == guid)
        .and_then(|input| input.unit_animation())
        .map(|owner| Rc::clone(owner.opacity_owner()))
        .ok_or_else(|| "passenger opacity owner".into())
}
