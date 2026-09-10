//! Native seat joins and the live model admission consumer.

use super::*;
use crate::application::terrain_frame::m2::M2TransparentDrawIndex;
use solarity_asset::VehicleCatalog;
use solarity_ecs::{
    WorldMovementContext, WorldMovementSpeeds, WorldMovementState, WorldMovementTransport,
};
use solarity_rendering::m2_model_distance_key;

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
