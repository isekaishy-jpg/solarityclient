//! Native corpse position, ownership, range and encrypted query response tests.

use super::{CorpseQuery, RuntimePlayerCorpse};
use crate::test_network::{TestError, WorldServer};
use glam::{Mat4, Vec3};
use solarity_ecs::{ActiveWorld, ObjectKind, WorldBootstrap, WorldMapId, WorldTransform};

fn unhex(value: &str) -> Result<Vec<u8>, TestError> {
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}
fn hex(value: &[u8]) -> String {
    value.iter().map(|b| format!("{b:02x}")).collect()
}
fn word(value: &str) -> Result<u32, TestError> {
    Ok(u32::from_str_radix(value, 16)?)
}
fn guid(value: &str) -> Result<u64, TestError> {
    Ok(u64::from_str_radix(value, 16)?)
}
fn floats<const N: usize>(value: &str) -> Result<[f32; N], TestError> {
    let values = unhex(value)?
        .as_chunks::<4>()
        .0
        .iter()
        .map(|v| f32::from_le_bytes(*v))
        .collect::<Vec<_>>();
    values.try_into().map_err(|_| "float count".into())
}
fn world(present: bool, ghost: bool, map: u32, position: Vec3) -> Result<ActiveWorld, TestError> {
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(map),
        7,
        "Corpse",
        position,
        0.0,
    ));
    let fields = [(24, 100), (32, 100), (150, if ghost { 16 } else { 0 })];
    world.create_object(7, ObjectKind::Player, None, fields)?;
    solarity_systems::project_object_fields(&mut world, 7, fields)?;
    if !present {
        let local = world.local_player();
        assert!(world.storage_mut().delete_entity(local));
    }
    Ok(world)
}
fn reset(maps: (u32, u32), in_range: bool) -> RuntimePlayerCorpse {
    let mut corpse = RuntimePlayerCorpse::default();
    corpse.ui.maps = maps;
    corpse.ui.in_range = in_range;
    corpse.position = Vec3::new(10., 20., 30.);
    corpse
}
fn event_id(event: Option<&str>) -> &str {
    match event {
        Some("CORPSE_IN_RANGE") => "184",
        Some("CORPSE_IN_INSTANCE") => "185",
        Some("CORPSE_OUT_OF_RANGE") => "186",
        None => "-",
        _ => "unknown",
    }
}
fn query_wire(corpse: &RuntimePlayerCorpse) -> String {
    if corpse.queries.is_empty() {
        return "-".into();
    }
    corpse
        .queries
        .iter()
        .map(|q| match q {
            CorpseQuery::Location => "16020000/".into(),
            CorpseQuery::Transport(counter) => format!("b6040000{}/", hex(&counter.to_le_bytes())),
        })
        .collect()
}
fn compare_state(
    corpse: &RuntimePlayerCorpse,
    event: Option<&str>,
    r: &[&str],
    line: &str,
) -> Result<(), TestError> {
    assert_eq!(corpse.ui.maps, (word(r[0])?, word(r[1])?), "{line}");
    assert_eq!(corpse.ui.in_range, r[2] == "1", "{line}");
    assert_eq!(corpse.position.to_array(), floats::<3>(r[3])?, "{line}");
    assert_eq!(corpse.transport_guid(), guid(r[4])?, "{line}");
    assert_eq!(event_id(event), r[5], "{line}");
    assert_eq!(query_wire(corpse), r[6], "{line}");
    if r[7] != "-" {
        assert_eq!(corpse.marker, floats::<2>(r[7])?, "{line}");
    }
    Ok(())
}

#[test]
fn corpse_ranges_and_transport_clocks_match_original_instructions() -> Result<(), TestError> {
    let mut count = 0;
    for line in include_str!("../fixtures/player_corpse_native.txt").lines() {
        let r = line.split_ascii_whitespace().collect::<Vec<_>>();
        match r[0] {
            "clear" => {
                let mut corpse = reset((0, 0), r[4] == "1");
                let event = corpse.clear(r[1] == "1" && r[2] == "16", r[3] == "4");
                compare_state(&corpse, event, &r[5..], line)?;
            }
            "range" => {
                let mut corpse = reset((word(r[2])?, word(r[3])?), r[6] == "1");
                let world = world(
                    r[1] == "1",
                    false,
                    r[4].parse()?,
                    Vec3::from_array(floats::<3>(r[7])?),
                )?;
                let event = corpse.advance(&world, r[5] == "4", 100);
                assert_eq!(corpse.ui.in_range, r[8] == "1", "{line}");
                assert_eq!(event_id(event), r[9], "{line}");
                if r[10] != "-" {
                    assert_eq!(corpse.marker, floats::<2>(r[10])?, "{line}");
                }
            }
            "transport" => {
                let mut corpse = reset((0, 0), false);
                corpse.transport = 9;
                corpse.query_deadline_seconds = word(r[3])?;
                corpse.fallback = Mat4::from_cols_array(&[
                    0., 1., 0., 0., -1., 0., 0., 0., 0., 0., 1., 0., 100., 200., 300., 1.,
                ]);
                let resident = (r[1] == "1").then(|| (r[2] == "1").then_some(corpse.fallback));
                let position = corpse.resolve_pose(resident, r[4].parse()?);
                assert_eq!(position.to_array(), floats::<3>(r[5])?, "{line}");
                assert_eq!(corpse.query_deadline_seconds, word(r[6])?, "{line}");
                assert_eq!(query_wire(&corpse), r[7], "{line}");
            }
            _ => continue,
        }
        count += 1;
    }
    assert_eq!(count, 396);
    Ok(())
}

#[test]
fn corpse_guid_matches_native_owned_non_bones_lifecycle() -> Result<(), TestError> {
    let mut count = 0;
    for line in include_str!("../fixtures/player_corpse_native.txt")
        .lines()
        .filter(|l| l.starts_with("owner "))
    {
        let r = line.split_ascii_whitespace().collect::<Vec<_>>();
        let mut world = world(true, false, 0, Vec3::ZERO)?;
        let original = 0x0000f10100000999;
        world.create_object(
            original,
            ObjectKind::Corpse,
            None,
            [(6, 7), (7, 0), (33, 0)],
        )?;
        let owner = guid(r[1])?;
        let flags = r[2].parse()?;
        let fields = [(6, owner as u32), (7, (owner >> 32) as u32), (33, flags)];
        let target = 0x0000f10112345678;
        if r[3] == "remove" {
            // Construct without ownership, then install raw fields before removal.
            // Values blocks never call the corpse constructor's setter.
            world.create_object(target, ObjectKind::Corpse, None, [(33, 1)])?;
            world.update_fields(target, fields)?;
            world.remove_object(target)?;
        } else {
            world.create_object(target, ObjectKind::Corpse, None, fields)?;
        }
        assert_eq!(world.local_corpse_guid(), guid(r[4])?, "{line}");
        count += 1;
    }
    assert_eq!(count, 40);
    // Removing an older owned corpse clears the last-created one's GUID.
    let mut world = world(true, false, 0, Vec3::ZERO)?;
    for guid in [8, 9] {
        world.create_object(guid, ObjectKind::Corpse, None, [(6, 7), (33, 0)])?;
    }
    assert_eq!(world.local_corpse_guid(), 9);
    world.remove_object(8)?;
    assert_eq!(world.local_corpse_guid(), 0);
    // Becoming bones, duplicate create, and removing bones retain the old value.
    world.create_object(10, ObjectKind::Corpse, None, [(6, 7), (33, 0)])?;
    world.update_fields(10, [(33, 1)])?;
    world.create_object(10, ObjectKind::Corpse, None, [(6, 8)])?;
    world.remove_object(10)?;
    assert_eq!(world.local_corpse_guid(), 10);
    Ok(())
}

#[test]
fn corpse_query_packets_match_native_through_encrypted_connection() -> Result<(), TestError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let (server, mut network) = WorldServer::connect().await?;
            let mut count = 0;
            for line in include_str!("../fixtures/player_corpse_native.txt").lines() {
                let r = line.split_ascii_whitespace().collect::<Vec<_>>();
                let (opcode, body) = match r[0] {
                    "packet" => (0x216, unhex(r[4])?),
                    "matrix" => (0x4b7, unhex(r[1])?),
                    _ => continue,
                };
                server
                    .exchange(vec![(opcode, body.clone())], 0)
                    .await?
                    .await??;
                let update = network
                    .receive_packet()
                    .await?
                    .player_corpse()?
                    .ok_or("corpse update")?;
                let mut corpse = reset((0, 0), true);
                if r[0] == "packet" {
                    let world = world(r[1] == "1", r[2] == "16", 0, Vec3::ZERO)?;
                    let event = corpse.receive(&world, update, r[3] == "4", 100);
                    compare_state(&corpse, event, &r[5..], line)?;
                } else {
                    let world = world(false, false, 0, Vec3::ZERO)?;
                    corpse.receive(&world, update, false, 100);
                    let expected = floats::<16>(r[2])?;
                    for (a, b) in corpse.fallback.to_cols_array().into_iter().zip(expected) {
                        assert!((a - b).abs() < 1e-7, "{a} {b}: {line}");
                    }
                }
                let malformed = (0..body.len())
                    .map(|n| (opcode, body[..n].to_vec()))
                    .collect();
                server.exchange(malformed, 0).await?.await??;
                for _ in 0..body.len() {
                    assert!(network.receive_packet().await?.player_corpse().is_err());
                }
                count += 1;
            }
            assert_eq!(count, 36);
            let (reader, mut writer) = network.split();
            let exchange = server.exchange_raw(Vec::new(), 2).await?;
            writer.send_corpse_query().await?;
            writer.send_corpse_transport_query(0x80000009).await?;
            assert_eq!(
                exchange.await??,
                vec![
                    (0x216, vec![]),
                    (0x4b6, 0x80000009_u32.to_le_bytes().to_vec())
                ]
            );
            drop(reader);
            Ok(())
        })
}

#[test]
fn corpse_live_transport_and_location_keep_independent_deadline_and_range() -> Result<(), TestError>
{
    let mut world = world(true, true, 0, Vec3::new(80., 210., 330.))?;
    let transport = 0x1fc0000000000009;
    world.create_object(
        transport,
        ObjectKind::GameObject,
        Some(WorldTransform::new(Vec3::ZERO, 0.)),
        [(4, 10), (5, 1f32.to_bits())],
    )?;
    solarity_systems::project_object_fields(&mut world, transport, [(4, 10), (5, 1f32.to_bits())])?;
    world.update_game_object_movement(transport, solarity_ecs::GameObjectMovement::new(0, None))?;
    let matrix = Mat4::from_cols_array(&[
        0., 1., 0., 0., -1., 0., 0., 0., 0., 0., 1., 0., 100., 200., 300., 1.,
    ]);
    world.update_game_object_animated_pose(
        transport,
        solarity_ecs::GameObjectAnimatedPose::new(matrix, 0),
    )?;
    let mut corpse = reset((0, 0), false);
    corpse.transport = 9;
    corpse.query_deadline_seconds = 130;
    corpse.ui.set_delay(30000, 1000);
    assert_eq!(corpse.advance(&world, false, 100), Some("CORPSE_IN_RANGE"));
    assert_eq!(corpse.marker, [80., 210.]);
    assert_eq!(corpse.query_deadline_seconds, 0);
    assert!(corpse.queries.is_empty());
    assert_eq!(corpse.ui.remaining(2000), 29);
    assert_eq!(corpse.clear(true, false), Some("CORPSE_OUT_OF_RANGE"));
    assert_eq!(corpse.ui.remaining(2000), 29);
    assert_eq!(corpse.queries.front(), Some(&CorpseQuery::Location));
    Ok(())
}

#[test]
fn ghost_field_notifications_preserve_native_corpse_event_order() -> Result<(), TestError> {
    use super::super::player_ui::{
        RuntimePlayerLifeEvent, RuntimePlayerUiNotification, RuntimePlayerUiState,
    };
    let mut count = 0;
    for line in include_str!("../fixtures/player_corpse_native.txt")
        .lines()
        .filter(|l| l.starts_with("flags "))
    {
        let r = line.split_ascii_whitespace().collect::<Vec<_>>();
        let world = world(true, r[2] == "16", 0, Vec3::ZERO)?;
        let mut state = RuntimePlayerUiState::default();
        state.arena = r[3] == "4";
        state.corpse = reset((0, 0), r[4] == "1");
        state.receive_unit_field(
            &world,
            world.object_identity(7).ok_or("player")?,
            crate::application::gameplay_session::UnitFieldNotification::PlayerFlags {
                previous: r[1].parse()?,
            },
            1000,
        );
        let mut events = Vec::new();
        while let Some(notification) = state.take_notification() {
            match notification {
                RuntimePlayerUiNotification::Life {
                    event: RuntimePlayerLifeEvent::Flags { unghost },
                    ..
                } => {
                    events.push("18f");
                    if unghost {
                        events.push("188");
                    }
                }
                RuntimePlayerUiNotification::CorpseLocation {
                    event: Some(event), ..
                } => events.push(event_id(Some(event))),
                RuntimePlayerUiNotification::CorpseLocation { event: None, .. }
                | RuntimePlayerUiNotification::Resurrection(_) => {}
                other => panic!("unexpected {other:?}: {line}"),
            }
        }
        assert_eq!(events.join(","), r[5], "{line}");
        assert_eq!(query_wire(&state.corpse), r[6], "{line}");
        count += 1;
    }
    assert_eq!(count, 16);
    Ok(())
}

#[test]
fn map_replacement_keeps_corpse_deadline_and_transport_fallback_until_next_entry()
-> Result<(), TestError> {
    let mut old = world(true, true, 0, Vec3::ZERO)?;
    old.create_object(9, ObjectKind::Corpse, None, [(6, 7), (33, 0)])?;
    let mut next = world(true, true, 1, Vec3::ZERO)?;
    next.inherit_removed_corpse_guid(&old);
    assert_eq!(next.local_corpse_guid(), 0);
    old.update_fields(9, [(33, 1)])?;
    next.inherit_removed_corpse_guid(&old);
    assert_eq!(next.local_corpse_guid(), 9);
    let mut state = super::super::player_ui::RuntimePlayerUiState::default();
    state.corpse.ui.set_delay(30_000, 1000);
    state.corpse.transport = 9;
    state.corpse.query_deadline_seconds = 130;
    state.corpse.fallback = Mat4::from_translation(Vec3::new(100., 200., 300.));
    state.clear_for_world_leave();
    state.corpse_world_entry(&next);
    assert_eq!(state.corpse.ui.remaining(2000), 29);
    assert_eq!(state.corpse.query_deadline_seconds, 130);
    assert_eq!(
        state.corpse.fallback.w_axis.truncate(),
        Vec3::new(100., 200., 300.)
    );
    assert_eq!(state.corpse.ui.maps, (u32::MAX, u32::MAX));
    state.corpse.queries.clear();
    state.corpse.receive(
        &next,
        solarity_network::WorldPlayerCorpseUpdate::Location {
            map: 1,
            position: [10., 20., 30.],
            display_map: 1,
            transport: 11,
        },
        false,
        100,
    );
    assert!(
        state.corpse.queries.is_empty(),
        "a new transport still obeys the retained native query deadline"
    );
    assert_eq!(state.corpse.marker, [110., 220.]);
    Ok(())
}
