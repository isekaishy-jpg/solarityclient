//! Native packet readers and attack GUID stores checked through encrypted packets.

use super::{
    apply_state_packet,
    player_ui::{RuntimePlayerUiNotification, RuntimePlayerUiState},
};
use crate::test_network::{TestError, WorldServer};
use solarity_ecs::{ActiveWorld, ObjectKind, WorldBootstrap, WorldMapId};

#[test]
fn creation_attack_state_matches_native_and_survives_movement_until_death() -> Result<(), TestError>
{
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let (server, mut network) = WorldServer::connect().await?;
            let mut world = ActiveWorld::enter(WorldBootstrap::new(
                WorldMapId::new(0),
                7,
                "WoundTest",
                glam::Vec3::ZERO,
                0.0,
            ));
            for line in include_str!("../fixtures/unit_wound_native.creates.txt")
                .lines()
                .filter(|line| !line.starts_with('#'))
            {
                let row = line.split_whitespace().collect::<Vec<_>>();
                let movement = row[0]
                    .as_bytes()
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
                    .collect::<Result<Vec<_>, TestError>>()?;
                let expected = u64::from_str_radix(row[1], 16)?;
                if world.entity_by_guid(9).is_some() {
                    world.remove_object(9)?;
                }
                for (guid, retained) in [(9, expected), (9, expected), (7, expected)] {
                    let mut body = vec![1, 0, 0, 0, 2, 1, guid, if guid == 7 { 4 } else { 3 }];
                    body.extend_from_slice(&movement);
                    body.push(0);
                    server.exchange(vec![(0xa9, body)], 0).await?.await??;
                    let packet = network.receive_packet().await?;
                    crate::application::gameplay_session::apply_object_updates(
                        &mut world,
                        &packet.object_updates()?.ok_or("create")?,
                    )?;
                    assert_eq!(
                        world.unit_attack_target(u64::from(guid)),
                        retained,
                        "{line}"
                    );
                }
            }
            world.set_unit_attack_target(9, 88);
            // A duplicate remote create has no authority over the live attack owner.
            server
                .exchange(vec![(0xa9, vec![1, 0, 0, 0, 2, 1, 9, 3, 0, 0, 0])], 0)
                .await?
                .await??;
            crate::application::gameplay_session::apply_object_updates(
                &mut world,
                &network
                    .receive_packet()
                    .await?
                    .object_updates()?
                    .ok_or("duplicate")?,
            )?;
            assert_eq!(world.unit_attack_target(9), 88);
            let mut movement = vec![1, 0, 0, 0, 1, 1, 9];
            movement.extend([0; 30]); // flags, timestamp, position/facing and fall time
            for speed in [2.5_f32, 7.0, 4.5, 4.75, 2.5, 7.0, 4.5, 3.0, 3.0] {
                movement.extend(speed.to_le_bytes());
            }
            server.exchange(vec![(0xa9, movement)], 0).await?.await??;
            crate::application::gameplay_session::apply_object_updates(
                &mut world,
                &network
                    .receive_packet()
                    .await?
                    .object_updates()?
                    .ok_or("movement")?,
            )?;
            assert_eq!(world.unit_attack_target(9), 88);
            world.update_fields(9, [(24, 100), (32, 100)])?;
            solarity_systems::project_object_fields(&mut world, 9, [(24, 100), (32, 100)])?;
            let mut state = RuntimePlayerUiState::default();
            state.receive_environmental_damage(
                &mut world,
                solarity_network::WorldEnvironmentalDamage {
                    guid: 9,
                    kind: 1,
                    amount: 10,
                    absorbed: 0,
                    resisted: 0,
                },
                None,
                None,
                Some(8),
                100,
            );
            world.set_unit_attack_target(9, 0);
            let impact = state.take_environmental_impact().ok_or("impact")?;
            assert_eq!(
                impact.attack_target_guid, 88,
                "later attack stop must not rewrite the damage packet context"
            );
            assert_eq!(impact.template_flags, Some(8));
            world.set_unit_attack_target(9, 88);
            let mut death = vec![1, 0, 0, 0, 0, 1, 9, 1];
            death.extend((1_u32 << 24).to_le_bytes());
            death.extend(0_u32.to_le_bytes());
            server.exchange(vec![(0xa9, death)], 0).await?.await??;
            crate::application::gameplay_session::apply_object_updates(
                &mut world,
                &network
                    .receive_packet()
                    .await?
                    .object_updates()?
                    .ok_or("death")?,
            )?;
            assert_eq!(
                world.unit_attack_target(9),
                0,
                "raw health death clears attack state"
            );
            Ok(())
        })
}

#[test]
fn encrypted_attack_packets_match_native_owner_and_notifications() -> Result<(), TestError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let (server, mut network) = WorldServer::connect().await?;
            let mut world = ActiveWorld::enter(WorldBootstrap::new(
                WorldMapId::new(0),
                7,
                "WoundTest",
                glam::Vec3::ZERO,
                0.0,
            ));
            world.create_object(7, ObjectKind::Player, None, [])?;
            let mut state = RuntimePlayerUiState::default();
            let mut count = 0;
            for line in include_str!("../fixtures/unit_wound_native.attacks.txt")
                .lines()
                .filter(|line| !line.starts_with('#'))
            {
                let row = line.split_whitespace().collect::<Vec<_>>();
                let opcode = u16::from_str_radix(row[0], 16)?;
                let body = row[1]
                    .as_bytes()
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
                    .collect::<Result<Vec<_>, TestError>>()?;
                let previous = u64::from_str_radix(row[2], 16)?;
                assert!(world.set_unit_attack_target(7, previous));
                server
                    .exchange(vec![(opcode, body.clone())], 0)
                    .await?
                    .await??;
                assert!(apply_state_packet(
                    &mut world,
                    &network.receive_packet().await?,
                    &mut state
                )?);
                assert_eq!(
                    world.unit_attack_target(7),
                    u64::from_str_radix(row[3], 16)?,
                    "{line}"
                );
                let event = match state.take_notification() {
                    Some(RuntimePlayerUiNotification::Attack(true)) => 155,
                    Some(RuntimePlayerUiNotification::Attack(false)) => 156,
                    None => -1,
                    value => panic!("unexpected notification {value:?}"),
                };
                assert_eq!(event, row[4].parse::<i32>()?, "{line}");
                assert!(state.take_notification().is_none());
                // A partial or overlong packet cannot mutate the retained owner.
                for length in 0..body.len() {
                    server
                        .exchange(vec![(opcode, body[..length].to_vec())], 0)
                        .await?
                        .await??;
                    assert!(network.receive_packet().await?.unit_attack().is_err());
                }
                let mut trailing = body;
                trailing.push(0);
                server
                    .exchange(vec![(opcode, trailing)], 0)
                    .await?
                    .await??;
                assert!(network.receive_packet().await?.unit_attack().is_err());
                count += 1;
            }
            assert_eq!(count, 7);
            world.create_object(9, ObjectKind::Unit, None, [])?;
            assert!(world.set_unit_attack_target(9, 88));
            world.remove_object(9)?;
            world.create_object(9, ObjectKind::Unit, None, [])?;
            assert_eq!(
                world.unit_attack_target(9),
                0,
                "replacement must own a new attack state"
            );
            Ok(())
        })
}
