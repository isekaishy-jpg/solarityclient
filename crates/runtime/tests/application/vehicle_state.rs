//! Encrypted object updates retain UnitVehicle_C's independent lifetime.

use crate::test_network::{TestError, WorldServer};
use solarity_ecs::{ActiveWorld, ObjectKind, WorldBootstrap, WorldMapId};

#[test]
fn vehicle_creation_payload_retains_facing_and_obeys_create_authority() -> Result<(), TestError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let (server, mut network) = WorldServer::connect().await?;
            let mut world = ActiveWorld::enter(WorldBootstrap::new(
                WorldMapId::new(0),
                7,
                "Vehicle",
                glam::Vec3::ZERO,
                0.0,
            ));
            // Each payload is followed by packed rotation and an update-field value;
            // neither tail may be displaced by the eight vehicle bytes.
            for (guid, kind, id, facing, expected) in [
                (9, 3, 1, 0.25_f32, Some((1, 0.25_f32))),
                (9, 3, 2, -0.5, Some((1, 0.25))), // remote duplicate is ignored
                (7, 4, 1, -0.75, Some((1, -0.75))),
                (7, 4, 0, 0.5, Some((0, -0.75))), // row replacement preserves facing
                (7, 4, u32::MAX, 1.5, Some((u32::MAX, -0.75))),
                (11, 3, 0, 0.0, Some((0, 0.0))), // zero still allocates an owner
                (12, 5, 1, 0.75, None),          // non-unit create never allocates UnitVehicle_C
            ] {
                let body = create(guid, kind, Some((id, facing)), true);
                server.exchange(vec![(0xa9, body)], 0).await?.await??;
                let updates = network
                    .receive_packet()
                    .await?
                    .object_updates()?
                    .ok_or("vehicle create")?;
                crate::application::gameplay_session::apply_object_updates(&mut world, &updates)?;
                assert_eq!(
                    world
                        .unit_vehicle(u64::from(guid))
                        .map(|v| (v.definition_id(), v.initial_facing())),
                    expected
                );
                let entity = world
                    .entity_by_guid(u64::from(guid))
                    .ok_or("created unit")?;
                assert_eq!(
                    world
                        .storage()
                        .get::<&solarity_ecs::ObjectFields>(entity)?
                        .get(4),
                    1.0_f32.to_bits()
                );
            }
            for guid in [7, 9] {
                let before = world.unit_vehicle(guid);
                server
                    .exchange(
                        vec![(
                            0xa9,
                            create(guid as u8, if guid == 7 { 4 } else { 3 }, None, false),
                        )],
                        0,
                    )
                    .await?
                    .await??;
                crate::application::gameplay_session::apply_object_updates(
                    &mut world,
                    &network
                        .receive_packet()
                        .await?
                        .object_updates()?
                        .ok_or("plain create")?,
                )?;
                assert_eq!(world.unit_vehicle(guid), before);
                let mut body = vec![1, 0, 0, 0, 1, 1, guid as u8];
                body.extend([0; 30]);
                for speed in [2.5_f32, 7.0, 4.5, 4.75, 2.5, 7.0, 4.5, 3.0, 3.0] {
                    body.extend(speed.to_le_bytes());
                }
                server.exchange(vec![(0xa9, body)], 0).await?.await??;
                crate::application::gameplay_session::apply_object_updates(
                    &mut world,
                    &network
                        .receive_packet()
                        .await?
                        .object_updates()?
                        .ok_or("movement")?,
                )?;
                assert_eq!(world.unit_vehicle(guid), before);
            }
            // A living create carries both MovementInfo pitch and the separate
            // initial vehicle facing; neither angle may overwrite the other.
            let mut living = vec![1, 0, 0, 0, 2, 1, 14, 3];
            living.extend(0x00a0_u16.to_le_bytes());
            living.extend(0x0020_0000_u32.to_le_bytes()); // swimming enables pitch
            living.extend(0_u16.to_le_bytes());
            living.extend(1234_u32.to_le_bytes());
            for value in [1.0_f32, 2.0, 3.0, 0.75, 0.125] {
                living.extend(value.to_le_bytes());
            }
            living.extend(0_u32.to_le_bytes()); // fall time
            for speed in [2.5_f32, 7.0, 4.5, 4.75, 2.5, 7.0, 4.5, 3.0, 3.0] {
                living.extend(speed.to_le_bytes());
            }
            living.extend(1_u32.to_le_bytes());
            living.extend((-0.25_f32).to_le_bytes());
            living.push(0); // no update fields
            server.exchange(vec![(0xa9, living)], 0).await?.await??;
            crate::application::gameplay_session::apply_object_updates(
                &mut world,
                &network
                    .receive_packet()
                    .await?
                    .object_updates()?
                    .ok_or("living vehicle")?,
            )?;
            assert_eq!(
                world.unit_vehicle(14).map(|v| v.initial_facing()),
                Some(-0.25)
            );
            assert_eq!(
                world
                    .movement_state(14)
                    .and_then(|m| m.context().pitch_radians),
                Some(0.125)
            );
            let original = world.object_identity(9);
            world.remove_object(9)?;
            assert_eq!(world.unit_vehicle(9), None);
            world.create_object(9, ObjectKind::Unit, None, [])?;
            assert_ne!(world.object_identity(9), original);
            assert_eq!(world.unit_vehicle(9), None);
            assert!(world.set_unit_vehicle(9, 2, -1.0));
            assert_eq!(
                world.unit_vehicle(9).map(|v| v.initial_facing()),
                Some(-1.0)
            );

            let body = create(13, 3, Some((1, 0.25)), true);
            for length in 0..body.len() {
                server
                    .exchange(vec![(0xa9, body[..length].to_vec())], 0)
                    .await?
                    .await??;
                assert!(
                    network.receive_packet().await?.object_updates().is_err(),
                    "length {length}"
                );
            }
            let mut trailing = body;
            trailing.push(0);
            server.exchange(vec![(0xa9, trailing)], 0).await?.await??;
            assert!(network.receive_packet().await?.object_updates().is_err());
            Ok(())
        })
}

fn create(guid: u8, kind: u8, vehicle: Option<(u32, f32)>, rotation: bool) -> Vec<u8> {
    let mut body = vec![1, 0, 0, 0, 2, 1, guid, kind];
    let flags = if vehicle.is_some() { 0x80_u16 } else { 0 } | if rotation { 0x200 } else { 0 };
    body.extend(flags.to_le_bytes());
    if let Some((id, facing)) = vehicle {
        body.extend(id.to_le_bytes());
        body.extend(facing.to_le_bytes());
    }
    if rotation {
        body.extend(0x1234_5678_9abc_def0_u64.to_le_bytes());
    }
    body.push(1);
    body.extend((1_u32 << 4).to_le_bytes());
    body.extend(1.0_f32.to_le_bytes());
    body
}
