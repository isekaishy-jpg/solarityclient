//! Seated CPU passengers own the resident vehicle's actual body or mount key.

use super::*;
use crate::application::unit_animation::{
    UnitMovementAnimationEvent, UnitMovementAnimationEventKind,
};

#[test]
fn vehicle_ride_owner_survives_passenger_model_arrival_and_shared_seat_departure()
-> Result<(), Box<dyn Error>> {
    let _sdl_guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let fixture = crate::test_support::unit_models::fixture_with_vehicle_ride_animation(115, 4)?;
    let platform = SdlPlatform::start(WindowConfiguration::new(128, 128, WindowMode::Windowed))?;
    let mut renderer = renderer(&platform)?;
    // Deliberately outside the world camera; ownership must run before culling.
    let camera = WorldCamera::orthographic(
        Vec3::new(10000., 0., 10.),
        Vec3::new(10000., 0., 0.),
        Vec3::Y,
        [-2., 2.],
        [-2., 2.],
        0.1,
        100.,
    )
    .frame(1.)?;
    for mounted in [false, true] {
        let mut presentation = unit_presentation(&fixture)?;
        let mut world = ActiveWorld::enter(WorldBootstrap::new(
            WorldMapId::new(0),
            7,
            "Local",
            Vec3::ZERO,
            0.,
        ));
        for guid in [20, 30] {
            add_unit(&mut world, guid, ObjectKind::Unit, 0)?;
        }
        world.create_object(
            10,
            ObjectKind::Unit,
            Some(WorldTransform::new(Vec3::ZERO, 0.)),
            [(4, 1_f32.to_bits())],
        )?;
        if mounted {
            equipment_residency::fields(&mut world, 30, &[(69, 102)])?;
        }
        world.set_unit_vehicle(30, 1, 0.);
        let parent = world.object_identity(30).ok_or("parent")?;
        let mut random = CrtRand::new();
        let mut frame = M2Frame::prepare(
            &mut renderer,
            &ResidentM2Scene::default(),
            fixture_animations(&fixture)?,
            &mut random,
            Arc::new(M2ParticleTwinkleTable::new(1)),
        )?;
        for now in [0, 100, 350, 400, 450, 500] {
            presentation.set_animation_scene_time(now);
            for guid in [10, 20] {
                let enter = now == 100;
                let leave = now == if guid == 20 { 450 } else { 500 };
                if !enter && !leave {
                    continue;
                }
                let movement = WorldMovementState::new(
                    if enter { 0x200 } else { 0 },
                    WorldMovementSpeeds::new([0.; 9]),
                    WorldMovementContext {
                        transport: enter.then_some(WorldMovementTransport {
                            guid: 30,
                            position: Vec3::ZERO,
                            orientation: 0.,
                            time_ms: now,
                            seat: 2,
                            interpolated_time_ms: None,
                        }),
                        ..Default::default()
                    },
                );
                world.update_movement(guid, movement)?;
                presentation.notify_movement_animation(UnitMovementAnimationEvent {
                    identity: world.object_identity(guid).ok_or("passenger")?,
                    movement,
                    stand: 0,
                    kind: UnitMovementAnimationEventKind::Passenger {
                        previous_transform: WorldTransform::new(Vec3::ZERO, 0.),
                        previous: leave.then_some((30, 2)),
                        parent: enter.then_some(parent),
                        animated: false,
                    },
                });
            }
            if now == 400 {
                equipment_residency::fields(
                    &mut world,
                    10,
                    &[
                        (4, 1_f32.to_bits()),
                        (23, u32::from_le_bytes([1, 1, 0, 0])),
                        (24, 100),
                        (32, 100),
                        (67, 100),
                        (68, 100),
                        (74, 0),
                    ],
                )?;
            }
            presentation.synchronize(Some(&world))?;
            presentation.synchronize_creatures(Some(&world), |_| None)?;
            frame
                .replace_creatures(
                    &mut renderer,
                    &presentation.resident_creature_frame_inputs(),
                    &mut random,
                )
                .map_err(|error| format!("replace mounted={mounted} now={now}: {error}"))?;
            frame
                .advance_unbound_passengers(
                    presentation.movement_animations(),
                    now as f32,
                    &mut random,
                )
                .map_err(|error| format!("unbound mounted={mounted} now={now}: {error}"))?;
            frame
                .update_creature_states(
                    &presentation.resident_creature_frame_inputs(),
                    now as f32,
                    &mut random,
                )
                .map_err(|error| format!("states mounted={mounted} now={now}: {error}"))?;
            equipment_residency::advance(&mut frame, &renderer, camera, now as f32, &mut random)
                .map_err(|error| format!("scene mounted={mounted} now={now}: {error}"))?;
            if now == 0 {
                continue;
            }
            assert_eq!(
                presentation.movement_animations().get(10).is_some(),
                now >= 400
            );
            let kind = if mounted {
                M2GpuPlacementOwner::CreatureMount { guid: 30 }
            } else {
                M2GpuPlacementOwner::CreatureBody { guid: 30 }
            };
            let placement = frame
                .placements
                .iter()
                .find(|placement| placement.owner == kind)
                .ok_or("vehicle model")?;
            let playback = placement
                .playback
                .as_ref()
                .ok_or("vehicle playback")?
                .borrow();
            let upper = playback.bone_playback(4).ok_or("vehicle controlled key")?;
            assert_eq!(upper.animation_id, 115, "mounted={mounted} now={now}");
            if now < 500 {
                assert_eq!(
                    upper.script_timer.ok_or("owned timer")?.unwrapped_time(now),
                    if now < 301 {
                        now.wrapping_sub(101)
                    } else {
                        now - 301
                    },
                    "mounted={mounted} now={now}"
                );
            } else {
                assert!(
                    upper.script_timer.is_none(),
                    "last passenger releases mounted={mounted}"
                );
            }
        }
    }
    Ok(())
}
