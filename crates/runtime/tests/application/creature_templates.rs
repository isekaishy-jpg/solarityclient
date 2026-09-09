//! Encrypted creature queries, exact unit callbacks, map replacement and writer pressure.

use glam::Vec3;
use solarity_ecs::{
    ActiveWorld, ObjectKind, ObjectPresentation, WorldBootstrap, WorldMapId, WorldObjectIdentity,
    WorldTransform,
};
use solarity_network::WorldTransfer;

use super::{ActiveGameplayNetwork, RuntimeGameplayCoordinator, WorldWriterCommand};
use crate::test_network::{TestError, WorldServer};

#[test]
fn creature_template_flags_follow_encrypted_replies_and_exact_unit_lifetimes()
-> Result<(), TestError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                let (server, mut network) = WorldServer::connect().await?;
                server
                    .exchange(vec![(0x61, template(88, 1 << 22))], 0)
                    .await?
                    .await??;
                let setup = network.receive_packet().await?;
                let mut gameplay = RuntimeGameplayCoordinator::new();
                gameplay.begin(&tokio::runtime::Handle::current(), network, vec![setup])?;
                let world = gameplay.world_mut().ok_or("world")?;
                let ready = unit(world, 88, 880)?;
                let retired = unit(world, 42, 420)?;
                let shared = unit(world, 42, 421)?;
                let missing = unit(world, 43, 430)?;
                let writes = server.exchange_raw(vec![], 2).await?;
                synchronize(&mut gameplay)?;
                assert_eq!(writes.await??, vec![request(42, 420), request(43, 430)]);
                assert_eq!(gameplay.unit_template_flags(ready), 1 << 22);
                assert_eq!(gameplay.unit_template_family(ready), Some(17));
                assert_eq!(gameplay.unit_template_family(shared), None);
                assert_eq!(gameplay.unit_template_flags(shared), 0);

                let world = gameplay.world_mut().ok_or("world")?;
                world.remove_object(420)?;
                let replacement = unit(world, 44, 420)?;
                assert_ne!(retired, replacement);
                let writes = server.exchange_raw(vec![], 1).await?;
                synchronize(&mut gameplay)?;
                assert_eq!(writes.await??, vec![request(44, 420)]);
                server
                    .exchange(
                        vec![
                            (0x61, template(42, 1 << 22)),
                            (0x61, (0x8000_0000_u32 | 43).to_le_bytes().to_vec()),
                            (0x61, template(44, 1 << 25)),
                        ],
                        0,
                    )
                    .await?
                    .await??;
                while gameplay.unit_template_flags(replacement) == 0 {
                    gameplay.service()?;
                    tokio::task::yield_now().await;
                }
                assert_eq!(gameplay.unit_template_flags(retired), 0);
                assert_eq!(gameplay.unit_template_family(retired), None);
                assert_eq!(gameplay.unit_template_family(shared), Some(17));
                assert_eq!(gameplay.unit_template_flags(shared), 1 << 22);
                assert_eq!(gameplay.unit_template_flags(replacement), 1 << 25);
                assert_eq!(gameplay.unit_template_flags(missing), 0);
                synchronize(&mut gameplay)?;
                assert!(gameplay.creature_templates.pending_request().is_none());

                // A real field packet changes the cache key. Dispatch must rebind
                // it without relying on the fixture's explicit synchronization.
                let mut update = vec![1, 0, 0, 0, 0, 3, 0xa5, 1, 1];
                update.extend(8_u32.to_le_bytes());
                update.extend(45_u32.to_le_bytes());
                server.exchange(vec![(0xa9, update)], 0).await?.await??;
                while gameplay.service()? == 0 {
                    tokio::task::yield_now().await;
                }
                assert_eq!(gameplay.unit_template_flags(shared), 0);
                let writes = server.exchange_raw(vec![], 1).await?;
                gameplay.send_creature_queries()?;
                assert_eq!(writes.await??, vec![request(45, 421)]);

                unit(gameplay.world_mut().ok_or("world")?, 43, 431)?;
                let writes = server.exchange_raw(vec![], 1).await?;
                synchronize(&mut gameplay)?;
                assert_eq!(writes.await??, vec![request(43, 431)]);
                server
                    .exchange(
                        vec![(
                            0x3e,
                            [530_u32, 32.0_f32.to_bits(), 0, 0, 0]
                                .into_iter()
                                .flat_map(u32::to_le_bytes)
                                .collect(),
                        )],
                        0,
                    )
                    .await?
                    .await??;
                let destination = loop {
                    gameplay.service()?;
                    if let Some(WorldTransfer::NewWorld(location)) = gameplay.take_world_transfer()
                    {
                        break location;
                    }
                    tokio::task::yield_now().await;
                };
                gameplay.replace_world(destination)?;
                let after_transfer = unit(gameplay.world_mut().ok_or("world")?, 42, 420)?;
                synchronize(&mut gameplay)?;
                assert_eq!(gameplay.unit_template_flags(after_transfer), 1 << 22);
                assert_eq!(gameplay.unit_template_flags(shared), 0);
                assert!(gameplay.creature_templates.pending_request().is_none());
                assert!(gameplay.unhandled_packets().is_empty());
                gameplay.disconnect();
                assert_eq!(gameplay.unit_template_flags(after_transfer), 0);
                Ok::<(), TestError>(())
            })
            .await?
        })
}

#[test]
fn creature_queries_retain_admission_order_under_writer_pressure() -> Result<(), TestError> {
    let runtime = tokio::runtime::Builder::new_current_thread().build()?;
    let (_sender, receiver) = tokio::sync::mpsc::channel(1);
    let (commands, mut writer) = tokio::sync::mpsc::channel(1);
    commands.try_send(WorldWriterCommand::ActiveMover(8))?;
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        1,
        "Fixture",
        Vec3::ZERO,
        0.,
    ));
    unit(&mut world, 11, 120)?;
    unit(&mut world, 12, 110)?;
    let mut gameplay = RuntimeGameplayCoordinator::with_test_world(world);
    gameplay.active = Some(ActiveGameplayNetwork {
        receiver,
        commands,
        task: runtime.spawn(std::future::pending()),
    });
    synchronize(&mut gameplay)?;
    assert_eq!(
        gameplay.creature_templates.pending_request(),
        Some((11, 120))
    );
    assert_eq!(writer.try_recv()?, WorldWriterCommand::ActiveMover(8));
    synchronize(&mut gameplay)?;
    assert_eq!(
        writer.try_recv()?,
        WorldWriterCommand::CreatureQuery {
            entry: 11,
            guid: 120
        }
    );
    assert_eq!(
        gameplay.creature_templates.pending_request(),
        Some((12, 110))
    );
    synchronize(&mut gameplay)?;
    assert_eq!(
        writer.try_recv()?,
        WorldWriterCommand::CreatureQuery {
            entry: 12,
            guid: 110
        }
    );
    synchronize(&mut gameplay)?;
    assert!(writer.try_recv().is_err());
    assert!(gameplay.creature_templates.pending_request().is_none());
    Ok(())
}

fn unit(world: &mut ActiveWorld, entry: u32, guid: u64) -> Result<WorldObjectIdentity, TestError> {
    let entity = world.create_object(
        guid,
        ObjectKind::Unit,
        Some(WorldTransform::new(Vec3::ZERO, 0.)),
        [],
    )?;
    world
        .storage_mut()
        .add_component(entity, (ObjectPresentation::new(entry, 1.),));
    world
        .object_identity(guid)
        .ok_or_else(|| "unit identity".into())
}

fn request(entry: u32, guid: u64) -> (u32, Vec<u8>) {
    (
        0x60,
        [
            entry.to_le_bytes().as_slice(),
            guid.to_le_bytes().as_slice(),
        ]
        .concat(),
    )
}

fn template(entry: u32, flags: u32) -> Vec<u8> {
    let mut body = entry.to_le_bytes().to_vec();
    body.extend([0; 6]);
    body.extend(flags.to_le_bytes());
    body.extend([0; 4]);
    body.extend(17_u32.to_le_bytes());
    body.extend([0; 28]);
    body.extend(1.0_f32.to_le_bytes());
    body.extend(1.0_f32.to_le_bytes());
    body.push(0);
    body.extend([0; 28]);
    body
}

// Fixtures edit ECS directly; live admission calls this after object-update dispatch.
fn synchronize(gameplay: &mut RuntimeGameplayCoordinator) -> Result<(), TestError> {
    gameplay
        .creature_templates
        .synchronize_world(gameplay.world.as_ref());
    gameplay.send_creature_queries()?;
    Ok(())
}
