//! Native death-log admission against encrypted creature-template replies.

use super::{
    CreatureTemplateCache, environmental_damage::RuntimeCombatLogClock,
    unit_death::RuntimeUnitDeathSnapshot,
};
use crate::test_network::{TestError, WorldServer};
use solarity_ecs::{ActiveWorld, ObjectKind, WorldBootstrap, WorldMapId};

#[test]
fn unit_death_log_admission_matches_original_template_and_lifetime_gate() -> Result<(), TestError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let (server, mut network) = WorldServer::connect().await?;
            for line in include_str!("../fixtures/unit_death_log_admission_native.txt").lines() {
                let row = line.split_ascii_whitespace().collect::<Vec<_>>();
                let player = row[0] == "19";
                let present = row[1] == "1";
                let flags = u32::from_str_radix(row[2], 16)?;
                let mut world = ActiveWorld::enter(WorldBootstrap::new(
                    WorldMapId::new(0),
                    1,
                    "WaterTest",
                    glam::Vec3::ZERO,
                    0.,
                ));
                world.create_object(1, ObjectKind::Player, None, [])?;
                let guid = if player { 2 } else { 0xf130000007000002 };
                let fields = [(3, 7), (24, 0), (32, 100)];
                world.create_object(
                    guid,
                    if player {
                        ObjectKind::Player
                    } else {
                        ObjectKind::Unit
                    },
                    None,
                    fields,
                )?;
                solarity_systems::project_object_fields(&mut world, guid, fields)?;
                let identity = world.object_identity(guid).ok_or("identity")?;
                let mut cache = CreatureTemplateCache::new();
                cache.synchronize_world(Some(&world));
                if present {
                    server
                        .exchange(vec![(0x61, template(flags, 13))], 0)
                        .await?
                        .await??;
                    cache.receive(
                        network
                            .receive_packet()
                            .await?
                            .creature_query()?
                            .ok_or("template")?,
                    );
                }
                let clock = RuntimeCombatLogClock {
                    unix_seconds: 1_700_000_000,
                    milliseconds: 1000,
                };
                let admitted = RuntimeUnitDeathSnapshot::admit(
                    &world,
                    identity,
                    Some(&cache),
                    None,
                    2250,
                    clock,
                );
                assert_eq!(admitted.is_some(), row[3] == "1", "{line}");
                if let Some(admitted) = admitted {
                    assert_eq!(
                        admitted.event,
                        if present {
                            "UNIT_DISSIPATES"
                        } else {
                            "UNIT_DIED"
                        }
                    );
                    assert_eq!(admitted.name.as_deref(), present.then_some("WaterTest"));
                    assert_eq!(admitted.flags, if player { 1320 } else { 2600 });
                }
                // A changed entry invalidates the old template even before cache synchronization.
                world.update_fields(guid, [(3, 8)])?;
                solarity_systems::project_object_fields(&mut world, guid, [(3, 8)])?;
                let changed = RuntimeUnitDeathSnapshot::admit(
                    &world,
                    identity,
                    Some(&cache),
                    None,
                    2250,
                    clock,
                );
                assert_eq!(changed.is_some(), player);
                if let Some(changed) = changed {
                    assert_eq!(changed.event, "UNIT_DIED");
                }
                world.remove_object(guid)?;
                world.create_object(guid, ObjectKind::Unit, None, fields)?;
                assert!(
                    RuntimeUnitDeathSnapshot::admit(
                        &world,
                        identity,
                        Some(&cache),
                        None,
                        2250,
                        clock
                    )
                    .is_none()
                );
            }
            Ok(())
        })
}

fn template(flags: u32, creature_type: u32) -> Vec<u8> {
    let mut body = 7_u32.to_le_bytes().to_vec();
    body.extend(b"WaterTest\0\0\0\0\0\0");
    body.extend(flags.to_le_bytes());
    body.extend(creature_type.to_le_bytes());
    body.extend([0; 32]);
    body.extend(1.0_f32.to_le_bytes());
    body.extend(1.0_f32.to_le_bytes());
    body.push(0);
    body.extend([0; 28]);
    body
}
