//! Native mount/player scales survive residency, attachment posing and dismounts.

use super::*;

/// 717910 gives the mount a separate model lifetime from rider materials.
#[test]
fn unchanged_mounts_retain_animation_and_effects_across_rider_rebuilds()
-> Result<(), Box<dyn Error>> {
    let _sdl_guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let fixture = crate::test_support::unit_models::fixture_with_mount_effects()?;
    let mut presentation = unit_presentation(&fixture)?;
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        Vec3::ZERO,
        0.,
    ));
    for guid in [7, 20] {
        add_unit(&mut world, guid, ObjectKind::Player, 0)?;
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
    let camera = WorldCamera::orthographic(
        Vec3::new(8., 0., 4.),
        Vec3::ZERO,
        Vec3::Z,
        [-8., 8.],
        [-8., 8.],
        0.1,
        100.,
    )
    .frame(1.)?;
    let publish =
        |presentation: &mut crate::application::player_coordinator::RuntimePlayerPresentation,
         world: &ActiveWorld,
         frame: &mut M2Frame,
         renderer: &mut VulkanRenderer,
         random: &mut CrtRand|
         -> Result<(), Box<dyn Error>> {
            presentation.synchronize(Some(world))?;
            presentation.synchronize_remote_players(Some(world))?;
            frame.replace_player(renderer, presentation.resident_frame_input(), random)?;
            frame.replace_remote_players(
                renderer,
                &presentation.resident_remote_player_frame_inputs(),
                random,
            )?;
            Ok(())
        };
    publish(
        &mut presentation,
        &world,
        &mut frame,
        &mut renderer,
        &mut random,
    )?;
    let mounts = [
        M2GpuPlacementOwner::PlayerMount { guid: 7 },
        M2GpuPlacementOwner::RemotePlayerMount { guid: 20 },
    ];
    let bodies = [
        M2GpuPlacementOwner::PlayerBody { guid: 7 },
        M2GpuPlacementOwner::RemotePlayerBody { guid: 20 },
    ];
    let created = mounts
        .iter()
        .map(|owner| effects(&frame, *owner).map(|state| state.last_update_ms))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .max()
        .ok_or("mount creation")?;
    for time in [created + 100, created + 300] {
        equipment_residency::advance(&mut frame, &renderer, camera, time as f32, &mut random)?;
    }
    let before = mounts
        .map(|owner| mount_snapshot(&frame, owner))
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    let body_sources = bodies
        .map(|owner| unit_source(&frame, owner))
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    for snapshot in &before {
        assert!(!snapshot.effects.particles.is_empty());
        assert!(snapshot.effects.ribbons.len() > 1);
        assert!(snapshot.event_time > 0.);
    }
    // An atlas rebuild and simultaneous movement must preserve instance state
    // while publishing the fresh parent transform before rider attachment posing.
    presentation.set_component_texture_level(
        solarity_rendering::CharacterComponentTextureLevel::new(8).ok_or("texture level")?,
    );
    let position = Vec3::new(1., 2., 0.5);
    for guid in [7, 20] {
        world.update_transform(guid, WorldTransform::new(position, 0.8))?;
    }
    let expected_random = random;
    publish(
        &mut presentation,
        &world,
        &mut frame,
        &mut renderer,
        &mut random,
    )?;
    assert_eq!(
        random, expected_random,
        "unchanged mounts consume no new variation rolls"
    );
    for (index, owner) in mounts.into_iter().enumerate() {
        assert_ne!(
            unit_source(&frame, bodies[index])?,
            body_sources[index],
            "rider resources rebuilt"
        );
        assert_eq!(mount_snapshot(&frame, owner)?, before[index], "{owner:?}");
        let mount = frame
            .placements
            .iter()
            .find(|p| p.owner == owner)
            .ok_or("mount")?;
        assert_eq!(mount.transform.w_axis.truncate(), position);
        assert_eq!(
            mount
                .ground_placement
                .as_ref()
                .ok_or("mount ground")?
                .position,
            position
        );
        let source = frame.sources[mount.source_index]
            .as_ref()
            .ok_or("retained mount source")?;
        assert_eq!(
            source.model.path(),
            &AssetPath::new("Creature/Alternate.m2")?
        );
    }
    equipment_residency::advance(
        &mut frame,
        &renderer,
        camera,
        (created + 350) as f32,
        &mut random,
    )?;
    for (index, owner) in mounts.into_iter().enumerate() {
        let after = mount_snapshot(&frame, owner)?;
        assert_eq!(
            after.effects.particle_allocation,
            before[index].effects.particle_allocation
        );
        assert!(
            after.effects.particles[0].age_seconds()
                > before[index].effects.particles[0].age_seconds()
        );
        assert!(after.event_time > before[index].event_time);
        let mount_index = frame
            .placements
            .iter()
            .position(|p| p.owner == owner)
            .ok_or("mount order")?;
        let body_index = frame
            .placements
            .iter()
            .position(|p| p.owner == bodies[index])
            .ok_or("rider order")?;
        assert!(
            mount_index < body_index,
            "retained parent is published before its rider"
        );
    }
    // 71C0E0 reads the current unit scale without replacing the mount model.
    // Repeated changes must update placement while preserving every live clock.
    let scaled_before = mounts
        .map(|owner| mount_snapshot(&frame, owner))
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    for object_scale in [1.3_f32, 0.75, 2.] {
        let previous_bodies = bodies
            .map(|owner| unit_source(&frame, owner))
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        for guid in [7, 20] {
            equipment_residency::fields(&mut world, guid, &[(4, object_scale.to_bits())])?;
        }
        let expected_random = random;
        publish(
            &mut presentation,
            &world,
            &mut frame,
            &mut renderer,
            &mut random,
        )?;
        for (index, owner) in mounts.into_iter().enumerate() {
            assert_eq!(
                mount_snapshot(&frame, owner)?,
                scaled_before[index],
                "scale {object_scale}, {owner:?}"
            );
            assert_ne!(unit_source(&frame, bodies[index])?, previous_bodies[index]);
            let mount = frame
                .placements
                .iter()
                .find(|p| p.owner == owner)
                .ok_or("scaled mount")?;
            let expected_scale = (0.5_f64 * f64::from(object_scale) * f64::from(1.6_f32)) as f32;
            assert_eq!(
                mount.ground_placement.as_ref().ok_or("ground")?.scale,
                expected_scale
            );
            for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                assert!(
                    (mount.transform.transform_vector3(axis).length() - expected_scale).abs()
                        < 1e-6
                );
            }
        }
        assert_eq!(
            random, expected_random,
            "size changes consume no mount rolls"
        );
    }
    // Reused GUIDs, changed displays, and remounts each begin a fresh instance.
    world.remove_object(20)?;
    add_unit(&mut world, 20, ObjectKind::Player, 0)?;
    equipment_residency::fields(&mut world, 20, &[(69, 102)])?;
    equipment_residency::fields(&mut world, 7, &[(69, 100)])?;
    publish(
        &mut presentation,
        &world,
        &mut frame,
        &mut renderer,
        &mut random,
    )?;
    for (index, owner) in mounts.into_iter().enumerate() {
        let new = mount_snapshot(&frame, owner)?;
        assert_ne!(new.source, before[index].source);
        assert!(new.effects.particles.is_empty());
        assert!(new.effects.ribbons.is_empty());
        assert_eq!(new.event_time, 0.);
    }
    let prior_source = unit_source(&frame, mounts[0])?;
    equipment_residency::fields(&mut world, 7, &[(69, 0)])?;
    publish(
        &mut presentation,
        &world,
        &mut frame,
        &mut renderer,
        &mut random,
    )?;
    assert!(!frame.placements.iter().any(|p| p.owner == mounts[0]));
    equipment_residency::fields(&mut world, 7, &[(69, 100)])?;
    publish(
        &mut presentation,
        &world,
        &mut frame,
        &mut renderer,
        &mut random,
    )?;
    assert_ne!(unit_source(&frame, mounts[0])?, prior_source);
    assert!(effects(&frame, mounts[0])?.particles.is_empty());
    Ok(())
}

/// Observable primary/event clocks and emitter history of one live mount.
#[derive(Debug, PartialEq)]
struct MountSnapshot {
    source: usize,
    sequence: usize,
    cycle: u32,
    cycle_start: f32,
    event_time: f32,
    global_event_time: f32,
    effects: UnitEffectsSnapshot,
}

fn mount_snapshot(
    frame: &M2Frame,
    owner: M2GpuPlacementOwner,
) -> Result<MountSnapshot, Box<dyn Error>> {
    let mount = frame
        .placements
        .iter()
        .find(|p| p.owner == owner)
        .ok_or("mount")?;
    let playback = mount.playback.as_ref().ok_or("mount playback")?.borrow();
    Ok(MountSnapshot {
        source: mount.source_index,
        sequence: playback.sequence,
        cycle: playback.cycle_count,
        cycle_start: playback.cycle_started_ms,
        event_time: playback.previous_event_elapsed_ms,
        global_event_time: playback.previous_global_event_elapsed_ms,
        effects: effects(frame, owner)?,
    })
}

/// Native 7197D0/82DD80 selects the mount's flags at unit scale, then the
/// attached rider inherits the tilted saddle basis and translation.
#[test]
fn mounted_ground_pose_reaches_local_and_remote_rider_attachments() -> Result<(), Box<dyn Error>> {
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
    for guid in [7, 20] {
        add_unit(&mut world, guid, ObjectKind::Player, 0)?;
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
    let camera = WorldCamera::orthographic(
        Vec3::new(8., 0., 4.),
        Vec3::ZERO,
        Vec3::Z,
        [-8., 8.],
        [-8., 8.],
        0.1,
        100.,
    )
    .frame(1.)?;
    let target = Vec3::new(-0.25, 0.15, 1.).normalize();
    let mut normal = solarity_rendering::M2GroundNormal::default();
    let mut previous_time = 0.;
    for (index, (time, mount_id, position, yaw)) in [
        (100., 102, Vec3::ZERO, 0.),
        (300., 102, Vec3::new(1., 2., 0.5), 0.8),
        (300., 102, Vec3::new(1., 2., 0.5), 0.8),
        (500., 0, Vec3::ZERO, 0.),
        (700., 102, Vec3::ZERO, 0.),
    ]
    .into_iter()
    .enumerate()
    {
        for guid in [7, 20] {
            world.update_fields(guid, [(69, mount_id)])?;
            solarity_systems::project_object_fields(&mut world, guid, [(69, mount_id)])?;
            world.update_transform(guid, WorldTransform::new(position, yaw))?;
            presentation.set_ground_normal(world.object_identity(guid).ok_or("identity")?, target);
        }
        presentation.synchronize(Some(&world))?;
        presentation.synchronize_remote_players(Some(&world))?;
        let local = presentation.resident_frame_input().ok_or("local")?;
        let remote = presentation.resident_remote_player_frame_inputs();
        frame.replace_player(&mut renderer, Some(local), &mut random)?;
        frame.replace_remote_players(&mut renderer, &remote, &mut random)?;
        if index != 0 {
            frame.update_player_state(
                presentation.resident_frame_input().ok_or("local")?,
                time,
                &mut random,
            )?;
            frame.update_remote_player_states(&remote, time, &mut random)?;
        }
        normal.advance(target, (time - previous_time) * 0.001)?;
        previous_time = time;
        frame.prepare_visible_draws(
            &renderer,
            WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
            camera,
            solarity_rendering::M2TransparentPass::One,
            Vec3::ZERO,
            time,
            M2CameraEffectScale::EXTERNAL_CAMERA,
            &mut random,
            None,
        )?;
        for (body_owner, mount_owner) in [
            (
                M2GpuPlacementOwner::PlayerBody { guid: 7 },
                M2GpuPlacementOwner::PlayerMount { guid: 7 },
            ),
            (
                M2GpuPlacementOwner::RemotePlayerBody { guid: 20 },
                M2GpuPlacementOwner::RemotePlayerMount { guid: 20 },
            ),
        ] {
            let body = frame
                .placements
                .iter()
                .find(|p| p.owner == body_owner)
                .ok_or("body")?;
            let yaw = body
                .unit_animation
                .as_ref()
                .ok_or("unit animation")?
                .body_pose()
                .placement_yaw;
            let expected_body = if mount_id == 0 {
                normal.transform(position, yaw, 0.5, 0, 0.)?
            } else {
                let mount = frame
                    .placements
                    .iter()
                    .find(|p| p.owner == mount_owner)
                    .ok_or("mount")?;
                let expected_mount = normal.transform(position, yaw, 0.8, 3, 0.)?;
                assert!(
                    mount.transform.abs_diff_eq(expected_mount, 1e-6),
                    "case {index}: {mount_owner:?}"
                );
                assert!(
                    mount.transform.z_axis.x < -0.08,
                    "mount receives slope normal"
                );
                expected_mount
                    * Mat4::from_translation(Vec3::new(0.25, 0.5, 1.))
                    * Mat4::from_scale(Vec3::splat(1. / 1.6))
            };
            assert!(
                body.transform.abs_diff_eq(expected_body, 1e-6),
                "case {index}: {body_owner:?}"
            );
        }
    }
    Ok(())
}

/// Original 73D5D0/71C0E0 records exercise both players with non-unit body,
/// display and model scales. The mount attachment must not resize its rider.
#[test]
fn native_mount_scales_reach_local_and_remote_rider_matrices() -> Result<(), Box<dyn Error>> {
    let _sdl_guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let fixture = crate::test_support::unit_models::fixture_with_mount_scale()?;
    let mut presentation = unit_presentation(&fixture)?;
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        Vec3::ZERO,
        0.0,
    ));
    for guid in [7, 20] {
        add_unit(&mut world, guid, ObjectKind::Player, 0)?;
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
    let camera = WorldCamera::orthographic(
        Vec3::new(8.0, 0.0, 0.0),
        Vec3::ZERO,
        Vec3::Z,
        [-4.0, 4.0],
        [-2.0, 2.0],
        0.1,
        100.0,
    )
    .frame(1.0)?;
    let native = include_bytes!("../fixtures/unit-mount-scale-native.bin");
    assert_eq!(&native[..8], b"UMS12340");
    let count = u32::from_le_bytes(native[8..12].try_into()?);
    assert_eq!(native.len(), 12 + count as usize * 32);
    for (index, record) in native[12..].as_chunks::<32>().0.iter().enumerate() {
        let mount_id = u32::from_le_bytes(record[..4].try_into()?);
        let mut values = [0.0_f32; 7];
        for (value, bytes) in values.iter_mut().zip(record[4..].as_chunks::<4>().0) {
            *value = f32::from_le_bytes(*bytes);
        }
        let [
            object_scale,
            display_scale,
            _,
            retained,
            reciprocal,
            model_scale,
            body_scale,
        ] = values;
        assert_eq!(retained.to_bits(), display_scale.to_bits());
        for guid in [7, 20] {
            let fields = [(4, object_scale.to_bits()), (69, mount_id)];
            world.update_fields(guid, fields)?;
            solarity_systems::project_object_fields(&mut world, guid, fields)?;
        }
        presentation.synchronize(Some(&world))?;
        presentation.synchronize_remote_players(Some(&world))?;
        let local = presentation.resident_frame_input().ok_or("local player")?;
        let remote = presentation.resident_remote_player_frame_inputs();
        for input in [&local, &remote[0]] {
            assert_eq!(input.object_scale().to_bits(), body_scale.to_bits());
            if let Some(mount) = input.mount() {
                assert_ne!(mount_id, 0);
                assert_eq!(mount.object_scale().to_bits(), model_scale.to_bits());
                assert_eq!(mount.rider_scale().to_bits(), reciprocal.to_bits());
            } else {
                assert_eq!(mount_id, 0);
            }
        }
        frame.replace_player(&mut renderer, Some(local), &mut random)?;
        frame.replace_remote_players(&mut renderer, &remote, &mut random)?;
        frame.prepare_visible_draws(
            &renderer,
            WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
            camera,
            solarity_rendering::M2TransparentPass::One,
            Vec3::ZERO,
            index as f32 * 100.0,
            M2CameraEffectScale::EXTERNAL_CAMERA,
            &mut random,
            None,
        )?;
        for (body, mount) in [
            (
                M2GpuPlacementOwner::PlayerBody { guid: 7 },
                M2GpuPlacementOwner::PlayerMount { guid: 7 },
            ),
            (
                M2GpuPlacementOwner::RemotePlayerBody { guid: 20 },
                M2GpuPlacementOwner::RemotePlayerMount { guid: 20 },
            ),
        ] {
            let rider = frame
                .placements
                .iter()
                .find(|placement| placement.owner == body)
                .ok_or("rider")?;
            // Stock stores a reciprocal before composing the attachment matrix;
            // the final axes retain that float rounding instead of recomputing a ratio.
            let expected = if mount_id == 0 {
                body_scale
            } else {
                model_scale * reciprocal
            };
            for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                let length = rider.transform.transform_vector3(axis).length();
                assert!(
                    (length - expected).abs() < 1e-6,
                    "case {index}: {body:?}: {length} != {expected}"
                );
            }
            let mount = frame
                .placements
                .iter()
                .find(|placement| placement.owner == mount);
            assert_eq!(mount.is_some(), mount_id != 0);
            if let Some(mount) = mount {
                for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                    assert!(
                        (mount.transform.transform_vector3(axis).length() - model_scale).abs()
                            < 1e-6
                    );
                }
            }
        }
    }
    Ok(())
}

/// Registration uses the mount's authored box and raw movement transform even
/// when neither model is drawn. Retained local/remote placements must update it.
#[test]
fn mounted_scene_callbacks_use_mount_bounds_and_follow_movement() -> Result<(), Box<dyn Error>> {
    use crate::application::terrain_coordinator::RuntimeTerrainCoordinator;
    use solarity_asset::MapCatalog;
    let _sdl_guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let fixture = crate::test_support::unit_models::fixture_with_mount_scale()?;
    let mut presentation = unit_presentation(&fixture)?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let maps = MapCatalog::load(&mut store)?;
    let mut terrain = RuntimeTerrainCoordinator::new(AssetStoreHandle::new(store), maps);
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        Vec3::ZERO,
        0.,
    ));
    for guid in [7, 20] {
        add_unit(&mut world, guid, ObjectKind::Player, 0)?;
    }
    terrain.synchronize(Some(&world))?;
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
    // Native outdoor depth ends at 2133.333...: the mounted leading corner
    // reaches 2132.9, while the body's reaches 2134.0 and is not admitted.
    let eye = Vec3::new(-2134.5, 0., 2.);
    let camera = WorldCamera::new(eye, eye + Vec3::X, Vec3::Z, 1., 0.1, 100.).frame(1.)?;
    let environment = solarity_systems::WorldEntityLightEnvironment::new(
        Vec3::splat(0.2),
        Vec3::splat(0.8),
        -Vec3::Z,
        -Vec3::Z,
    );
    for (index, (mount, x, yaw, admitted)) in [
        (102, 0., 0., true),
        (102, 0., std::f32::consts::FRAC_PI_2, false),
        (102, 0., 0., true),
        (102, 5., 0., false),
        (102, 0., 0., true),
        (0, 0., 0., false),
        (102, 0., 0., true),
    ]
    .into_iter()
    .enumerate()
    {
        for guid in [7, 20] {
            world.update_fields(guid, [(69, mount)])?;
            solarity_systems::project_object_fields(&mut world, guid, [(69, mount)])?;
            world.update_transform(guid, WorldTransform::new(Vec3::new(x, 0., 0.), yaw))?;
        }
        presentation.synchronize(Some(&world))?;
        presentation.synchronize_remote_players(Some(&world))?;
        let local = presentation.resident_frame_input().ok_or("local")?;
        let remote = presentation.resident_remote_player_frame_inputs();
        frame.replace_player(&mut renderer, Some(local), &mut random)?;
        frame.replace_remote_players(&mut renderer, &remote, &mut random)?;
        let time = index as f32 * 100.;
        if index != 0 {
            frame.update_player_state(
                presentation.resident_frame_input().ok_or("local")?,
                time,
                &mut random,
            )?;
            frame.update_remote_player_states(&remote, time, &mut random)?;
        }
        let draws = frame.prepare_visible_draws_with_unit_effects(
            &renderer,
            WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
            camera,
            solarity_rendering::M2TransparentPass::One,
            Vec3::ZERO,
            time,
            M2CameraEffectScale::EXTERNAL_CAMERA,
            &mut random,
            None,
            None,
            None,
            Some((&mut terrain, environment)),
        )?;
        assert!(
            draws.draws.is_empty(),
            "outdoor callbacks precede draw culling"
        );
        for guid in [7, 20] {
            let identity = world.object_identity(guid).ok_or("identity")?;
            assert_eq!(
                presentation.take_scene_collision(identity),
                admitted,
                "case {index}, unit {guid}"
            );
            assert!(
                !presentation.take_scene_collision(identity),
                "single callback latch"
            );
        }
    }
    Ok(())
}
