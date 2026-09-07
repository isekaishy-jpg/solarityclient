//! Stock cache admission, encrypted replies, callback lifetime, and writer pressure.

use std::rc::Rc;

use solarity_network::WorldTransfer;

use super::{ActiveGameplayNetwork, RuntimeGameplayCoordinator, WorldWriterCommand};
use crate::test_network::{TestError, WorldServer};

/// Same-entry models share one query and template; map replacement preserves
/// the cache while a missing response permits only a later new admission.
#[test]
fn game_object_templates_follow_session_and_object_lifetimes() -> Result<(), TestError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                let (server, mut network) = WorldServer::connect().await?;
                server
                    .exchange(vec![(0x5f, template(88))], 0)
                    .await?
                    .await??;
                let setup = network.receive_packet().await?;
                let mut gameplay = RuntimeGameplayCoordinator::new();
                gameplay.begin(&tokio::runtime::Handle::current(), network, vec![setup])?;
                let setup = gameplay.game_object_templates_mut().bind(88, 888);
                assert_eq!(setup.template().ok_or("setup template")?.entry(), 88);
                let first = gameplay.game_object_templates_mut().bind(42, 420);
                let second = gameplay.game_object_templates_mut().bind(42, 421);
                let retired = gameplay.game_object_templates_mut().bind(42, 422);
                drop(retired);
                let writes = server.exchange_raw(vec![], 1).await?;
                gameplay.send_game_object_queries()?;
                let packets = writes.await??;
                assert_eq!(
                    packets,
                    vec![(
                        0x5e,
                        [
                            42_u32.to_le_bytes().as_slice(),
                            420_u64.to_le_bytes().as_slice()
                        ]
                        .concat()
                    )]
                );
                assert!(
                    gameplay
                        .game_object_templates_mut()
                        .pending_request()
                        .is_none()
                );
                server
                    .exchange(vec![(0x5f, template(42))], 0)
                    .await?
                    .await??;
                while first.template().is_none() {
                    gameplay.service()?;
                    tokio::task::yield_now().await;
                }
                let first_template = first.template().ok_or("first template")?;
                assert!(Rc::ptr_eq(
                    &first_template,
                    &second.template().ok_or("second template")?
                ));
                assert_eq!(first_template.properties()[23], 23);
                let missing = gameplay.game_object_templates_mut().bind(43, 430);
                let writes = server.exchange_raw(vec![], 1).await?;
                gameplay.send_game_object_queries()?;
                assert_eq!(writes.await??.len(), 1);
                // The following transfer is an ordered dispatch barrier after the reply.
                server
                    .exchange(
                        vec![
                            (0x5f, (0x8000_0000_u32 | 43).to_le_bytes().to_vec()),
                            (
                                0x3e,
                                [530_u32, 32.0_f32.to_bits(), 0, 0, 0]
                                    .into_iter()
                                    .flat_map(u32::to_le_bytes)
                                    .collect(),
                            ),
                        ],
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
                assert!(missing.template().is_none());
                assert!(
                    gameplay
                        .game_object_templates_mut()
                        .pending_request()
                        .is_none()
                );
                gameplay.replace_world(destination)?;
                let replacement = gameplay.game_object_templates_mut().bind(42, 420);
                assert!(Rc::ptr_eq(
                    &first_template,
                    &replacement.template().ok_or("replacement template")?
                ));
                assert!(
                    gameplay
                        .game_object_templates_mut()
                        .pending_request()
                        .is_none()
                );
                let later_missing = gameplay.game_object_templates_mut().bind(43, 431);
                assert!(later_missing.template().is_none());
                assert_eq!(
                    gameplay.game_object_templates_mut().pending_request(),
                    Some((43, 431))
                );
                assert!(gameplay.unhandled_packets().is_empty());
                gameplay.disconnect();
                let fresh_session = gameplay.game_object_templates_mut().bind(42, 420);
                assert!(fresh_session.template().is_none());
                assert_eq!(
                    gameplay.game_object_templates_mut().pending_request(),
                    Some((42, 420))
                );
                Ok::<(), TestError>(())
            })
            .await?
        })
}

/// A full writer retains the exact oldest query and does not enqueue duplicates.
#[test]
fn game_object_template_queries_preserve_order_under_writer_backpressure() -> Result<(), TestError>
{
    let runtime = tokio::runtime::Builder::new_current_thread().build()?;
    let (_sender, receiver) = tokio::sync::mpsc::channel(1);
    let (commands, mut writer) = tokio::sync::mpsc::channel(1);
    commands.try_send(WorldWriterCommand::ActiveMover(8))?;
    let mut gameplay = RuntimeGameplayCoordinator::new();
    gameplay.active = Some(ActiveGameplayNetwork {
        receiver,
        commands,
        task: runtime.spawn(std::future::pending()),
    });
    let first = gameplay.game_object_templates_mut().bind(11, 110);
    let second = gameplay.game_object_templates_mut().bind(12, 120);
    gameplay.send_game_object_queries()?;
    assert_eq!(
        gameplay.game_object_templates_mut().pending_request(),
        Some((11, 110))
    );
    assert_eq!(writer.try_recv()?, WorldWriterCommand::ActiveMover(8));
    gameplay.send_game_object_queries()?;
    assert_eq!(
        writer.try_recv()?,
        WorldWriterCommand::GameObjectQuery {
            entry: 11,
            guid: 110
        }
    );
    assert_eq!(
        gameplay.game_object_templates_mut().pending_request(),
        Some((12, 120))
    );
    gameplay.send_game_object_queries()?;
    assert_eq!(
        writer.try_recv()?,
        WorldWriterCommand::GameObjectQuery {
            entry: 12,
            guid: 120
        }
    );
    assert!(
        gameplay
            .game_object_templates_mut()
            .pending_request()
            .is_none()
    );
    assert!(first.template().is_none());
    assert!(second.template().is_none());
    Ok(())
}

/// Native 0x0098D750 template image with all properties and empty byte strings.
fn template(entry: u32) -> Vec<u8> {
    let mut body: Vec<_> = [entry, 15, 3015]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    body.extend([0; 7]);
    body.extend((0_u32..24).flat_map(u32::to_le_bytes));
    body.extend(1.0_f32.to_le_bytes());
    body.extend([0; 24]);
    body
}
