//! Type-11 creation, asynchronous admission, reversal, and passenger frame behavior.

use std::error::Error;

use glam::Vec3;
use solarity_ecs::{ActiveWorld, WorldTransform};

use super::{fields, presentation_for_route_with_files, table, template_for_type, world};
use crate::application::game_object_behavior::GameObjectNotification;
use crate::application::game_object_coordinator::RuntimeGameObjectPresentation;
use crate::application::gameplay_coordinator::GameObjectTemplateCache;
use crate::application::gameplay_session::apply_object_updates_with;
use crate::random::CrtRand;
use crate::test_network::{TestError, WorldServer};

/// A linear up/down animation with independently keyed quaternion tilt.
fn presentation(
    positions: &[(u32, f32, u32)],
    rotations: &[(u32, [f32; 4])],
) -> Result<RuntimeGameObjectPresentation, Box<dyn Error>> {
    let positions = table(
        7,
        &positions
            .iter()
            .enumerate()
            .map(|(index, &(time, x, sequence))| {
                vec![index as u32 + 1, 42, time, x.to_bits(), 0, 0, sequence]
            })
            .collect::<Vec<_>>(),
    );
    let rotations = table(
        7,
        &rotations
            .iter()
            .enumerate()
            .map(|(index, &(time, rotation))| {
                let mut row = vec![index as u32 + 1, 42, time];
                row.extend(rotation.map(f32::to_bits));
                row
            })
            .collect::<Vec<_>>(),
    );
    presentation_for_route_with_files(
        &[],
        &[
            ("DBFilesClient\\TransportAnimation.dbc", &positions),
            ("DBFilesClient\\TransportRotation.dbc", &rotations),
        ],
    )
}

/// The real behavior constructor receives its creation clock before any template.
fn initialize(
    objects: &mut RuntimeGameObjectPresentation,
    world: &mut ActiveWorld,
    split_ms: u32,
    progress: u16,
) -> Result<(), Box<dyn Error>> {
    fields(
        world,
        &[
            (13, 1_f32.to_bits()),
            (14, u32::from(progress) << 16),
            (16, split_ms),
            (17, 11 << 8),
        ],
    )?;
    world.update_transform(9, WorldTransform::new(Vec3::ZERO, 0.))?;
    let identity = world.object_identity(9).ok_or("transport identity")?;
    objects.observe_notification(
        world,
        identity,
        GameObjectNotification::Initialize,
        100,
        &mut CrtRand::new(),
    )?;
    Ok(())
}

fn position(world: &ActiveWorld) -> Result<f32, Box<dyn Error>> {
    Ok(world
        .game_object_animated_pose(9)
        .ok_or("animated pose")?
        .matrix()
        .w_axis
        .x)
}

#[test]
fn construction_and_receipt_reversal_run_before_resource_and_template_admission()
-> Result<(), Box<dyn Error>> {
    let mut objects = presentation(&[(0, 0., 0), (500, 10., 162), (1000, 0., 0)], &[])?;
    let mut world = world(400, 0)?;
    initialize(&mut objects, &mut world, 400, 32768)?;
    assert_eq!(position(&world)?, 4.);
    let identity = world.object_identity(9).ok_or("identity")?;
    // 710820 captures receipt raw time zero and reverses the half-complete
    // state-zero interval; a progress notification does not reset that anchor.
    fields(&mut world, &[(14, 0xffff_0000), (17, (11 << 8) | 1)])?;
    let mut random = CrtRand::new();
    for event in [
        GameObjectNotification::Progress,
        GameObjectNotification::State,
    ] {
        objects.observe_notification(&mut world, identity, event, 100, &mut random)?;
    }
    // GO+204 filters this repeated state even though the clock's cached state is 0.
    objects.observe_notification(
        &mut world,
        identity,
        GameObjectNotification::State,
        150,
        &mut random,
    )?;
    objects.synchronize(Some(&world))?;
    assert!(objects.game_object_template(9).is_none());
    objects.advance_transports(Some(&mut world), 200, 100)?;
    assert_eq!(position(&world)?, 2.);
    // Passenger phase uses the retained state before the actual geometry sample.
    assert_eq!(objects.object_passenger_time_ms(identity), Some(300));
    objects.advance_transports(Some(&mut world), 300, 100)?;
    assert_eq!(position(&world)?, 0.);
    assert_eq!(objects.object_passenger_time_ms(identity), Some(400));
    objects.advance_transports(Some(&mut world), 301, 1)?;
    assert_eq!(objects.object_passenger_time_ms(identity), Some(0));
    Ok(())
}

#[test]
fn loaded_empty_track_preserves_one_frame_of_published_passenger_time() -> Result<(), Box<dyn Error>>
{
    let mut objects = presentation(&[], &[])?;
    let mut world = world(0, 0)?;
    initialize(&mut objects, &mut world, 0, 0)?;
    objects.synchronize(Some(&world))?;
    let identity = world.object_identity(9).ok_or("identity")?;
    objects.advance_transports(Some(&mut world), 200, 100)?;
    assert_eq!(objects.object_passenger_time_ms(identity), Some(100));
    let mut cache = GameObjectTemplateCache::new();
    cache.receive(template_for_type(11, 0)?);
    objects.synchronize_templates(&mut cache);
    objects.advance_transports(Some(&mut world), 300, 100)?;
    assert_eq!(objects.object_passenger_time_ms(identity), Some(100));
    objects.advance_transports(Some(&mut world), 400, 100)?;
    assert_eq!(objects.object_passenger_time_ms(identity), Some(300));
    // 6F1490 does not dispatch another frame at the same or earlier signed time.
    objects.advance_transports(Some(&mut world), 500, 0)?;
    assert_eq!(objects.object_passenger_time_ms(identity), Some(300));
    objects.advance_transports(Some(&mut world), 500, u32::MAX)?;
    assert_eq!(objects.object_passenger_time_ms(identity), Some(300));
    Ok(())
}

#[test]
fn tiny_motion_accumulates_only_for_linked_passengers_and_stalls_presample()
-> Result<(), Box<dyn Error>> {
    for occupied in [false, true] {
        let amplitude = 1.0 / 262_144.0;
        let mut objects = presentation(&[(0, 0., 0), (1000, amplitude, 0)], &[])?;
        let mut world = world(0, 0)?;
        world.remove_object(10)?;
        initialize(&mut objects, &mut world, 0, 0)?;
        objects.synchronize(Some(&world))?;
        let identity = world.object_identity(9).ok_or("identity")?;
        objects.synchronize_transport_passengers(occupied.then_some(identity));
        objects.advance_transports(Some(&mut world), 200, 100)?;
        assert_eq!(
            position(&world)?,
            if occupied { 0. } else { amplitude * 0.1 }
        );
        objects.advance_transports(Some(&mut world), 300, 100)?;
        assert_eq!(
            position(&world)?,
            if occupied { 0. } else { amplitude * 0.2 }
        );
        objects.advance_transports(Some(&mut world), 400, 100)?;
        assert_eq!(position(&world)?, amplitude * 0.3);
    }
    let amplitude = 1.0 / 524_288.0;
    let mut objects = presentation(&[(0, 0., 0), (1000, amplitude, 0)], &[])?;
    let mut world = world(0, 0)?;
    initialize(&mut objects, &mut world, 0, 0)?;
    objects.synchronize(Some(&world))?;
    let identity = world.object_identity(9).ok_or("identity")?;
    objects.synchronize_transport_passengers([identity]);
    objects.advance_transports(Some(&mut world), 1000, 900)?;
    // 7139E0 moves the cached position to raw time 650 before measuring the last
    // 250 ms. Its remaining displacement is below the linked-passenger epsilon.
    assert_eq!(position(&world)?, amplitude * 0.65);
    assert_eq!(objects.object_passenger_time_ms(identity), Some(900));
    Ok(())
}

#[test]
fn late_map_admission_uses_the_current_full_animated_matrix_and_key_sequence()
-> Result<(), Box<dyn Error>> {
    let half = std::f32::consts::FRAC_1_SQRT_2;
    let mut objects = presentation(
        &[(0, 0., 506), (500, 10., 162), (1000, 0., 0)],
        &[
            (0, [0., 0., 0., 1.]),
            (500, [half, 0., 0., half]),
            (1000, [0., 0., 0., 1.]),
        ],
    )?;
    let mut world = world(0, 0)?;
    initialize(&mut objects, &mut world, 0, 0)?;
    objects.synchronize(Some(&world))?;
    objects.advance_transports(Some(&mut world), 600, 500)?;
    let identity = world.object_identity(9).ok_or("identity")?;
    let instance = objects.movement_instance(identity).ok_or("instance")?;
    assert!(instance.map_placement().is_none());
    assert_eq!(
        instance
            .transport
            .as_ref()
            .ok_or("behavior")?
            .animation_phase(),
        None
    );
    let mut cache = GameObjectTemplateCache::new();
    cache.receive(template_for_type(11, 0)?);
    objects.synchronize_templates(&mut cache);
    objects.advance_transports(Some(&mut world), 601, 1)?;
    let instance = objects.movement_instance(identity).ok_or("instance")?;
    let placement = instance.map_placement().ok_or("map placement")?;
    assert_eq!(
        placement.matrix(),
        world.game_object_animated_pose(9).ok_or("pose")?.matrix()
    );
    assert!(placement.matrix().y_axis.z.abs() > 0.99);
    assert_eq!(
        instance
            .transport
            .as_ref()
            .ok_or("behavior")?
            .animation_phase(),
        Some(162)
    );
    objects.synchronize_animations(Some(&world), &mut CrtRand::new())?;
    Ok(())
}

#[test]
fn one_encrypted_packet_constructs_the_animation_before_its_later_state_block()
-> Result<(), Box<dyn Error>> {
    let mut objects = presentation(&[(0, 0., 0), (500, 10., 162), (1000, 0., 0)], &[])?;
    let mut world = world(0, 0)?;
    world.remove_object(10)?;
    world.remove_object(9)?;
    let result: Result<(), TestError> = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let (server, mut session) = WorldServer::connect().await?;
            let mut body = 2_u32.to_le_bytes().to_vec();
            body.extend([2, 1, 9, 5]);
            body.extend(0x242_u16.to_le_bytes());
            body.extend([0; 16]);
            body.extend(123_u32.to_le_bytes());
            body.extend(0_u64.to_le_bytes());
            body.push(1);
            body.extend(
                [3, 4, 8, 14, 16, 17]
                    .into_iter()
                    .fold(0_u32, |mask, field| mask | (1 << field))
                    .to_le_bytes(),
            );
            body.extend(
                [42, 1_f32.to_bits(), 42, 0x8000_0000, 400, 11 << 8]
                    .into_iter()
                    .flat_map(u32::to_le_bytes),
            );
            body.extend([0, 1, 9, 1]); // values, packed GUID, one mask word
            body.extend(((1_u32 << 14) | (1 << 17)).to_le_bytes());
            body.extend(
                [0xffff_0000, (11 << 8) | 1]
                    .into_iter()
                    .flat_map(u32::to_le_bytes),
            );
            server.exchange(vec![(0xa9, body)], 0).await?.await??;
            let packet = session.receive_packet().await?;
            let updates = packet.object_updates()?.ok_or("object update batch")?;
            apply_object_updates_with::<TestError>(
                &mut world,
                &updates,
                1000,
                &mut |world, identity, event| {
                    objects.observe_notification(
                        world,
                        identity,
                        event,
                        1000,
                        &mut CrtRand::new(),
                    )?;
                    Ok(())
                },
            )?;
            Ok(())
        });
    result.map_err(|error| error as Box<dyn Error>)?;
    assert_eq!(position(&world)?, 4.);
    objects.synchronize(Some(&world))?;
    objects.advance_transports(Some(&mut world), 1100, 100)?;
    assert_eq!(position(&world)?, 2.);
    Ok(())
}

#[test]
fn a_long_frame_consumes_both_key_requests_and_preserves_the_initial_506_sentinel()
-> Result<(), Box<dyn Error>> {
    let mut objects = presentation(
        &[(0, 0., 506), (500, 5., 162), (800, 8., 164), (1000, 10., 0)],
        &[],
    )?;
    let mut world = world(0, 0)?;
    initialize(&mut objects, &mut world, 0, 0)?;
    objects.synchronize(Some(&world))?;
    let mut cache = GameObjectTemplateCache::new();
    cache.receive(template_for_type(11, 0)?);
    objects.synchronize_templates(&mut cache);
    objects.advance_transports(Some(&mut world), 100, 1)?;
    let mut random = CrtRand::new();
    objects.synchronize_animations(Some(&world), &mut random)?;
    let identity = world.object_identity(9).ok_or("identity")?;
    let instance = objects.movement_instance(identity).ok_or("instance")?;
    assert_eq!(
        instance
            .transport
            .as_ref()
            .ok_or("behavior")?
            .animation_phase(),
        None
    );
    let playback = instance
        .transport_model()
        .ok_or("model")?
        .playback()
        .ok_or("playback")?;
    assert_eq!(playback.borrow().animation_id, 0);
    let mut expected = random;
    // Native selection and cycle-count rolls each consume one CRT value.
    for _ in 0..4 {
        let _ = expected.next_u15();
    }
    objects.advance_transports(Some(&mut world), 1000, 900)?;
    objects.synchronize_animations(Some(&world), &mut random)?;
    assert_eq!(playback.borrow().animation_id, 164);
    assert_eq!(random, expected);
    objects.synchronize_animations(Some(&world), &mut random)?;
    assert_eq!(random, expected);
    Ok(())
}

#[test]
fn game_object_passengers_enable_the_threshold_and_retire_with_their_lifetime()
-> Result<(), Box<dyn Error>> {
    let amplitude = 1.0 / 262_144.0;
    let mut objects = presentation(&[(0, 0., 0), (1000, amplitude, 0)], &[])?;
    let mut world = world(0, 0)?;
    initialize(&mut objects, &mut world, 0, 0)?;
    objects.synchronize(Some(&world))?;
    objects.advance_transports(Some(&mut world), 200, 100)?;
    assert_eq!(position(&world)?, 0.);
    world.remove_object(10)?;
    objects.synchronize(Some(&world))?;
    objects.advance_transports(Some(&mut world), 300, 100)?;
    assert_eq!(position(&world)?, amplitude * 0.2);
    Ok(())
}
