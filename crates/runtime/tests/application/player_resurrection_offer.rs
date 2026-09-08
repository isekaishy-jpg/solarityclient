//! Offer packet decoding and ordered native death/ghost admission.

use super::player_ui::{RuntimePlayerUiNotification, RuntimePlayerUiState};
use crate::test_network::{TestError, WorldServer};
use solarity_ecs::{ActiveWorld, ObjectKind, WorldBootstrap, WorldMapId};

fn unhex(value: &str) -> Result<Vec<u8>, TestError> {
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}

fn named(guid: u64, name: &str) -> solarity_network::WorldPlayerNameResponse {
    solarity_network::WorldPlayerNameResponse {
        guid,
        result: solarity_network::WorldPlayerNameResult::Found(solarity_network::WorldPlayerName {
            name: name.into(),
            realm: String::new(),
            race: 0,
            gender: 0,
            class: 0,
            declined: None,
            flagged: false,
        }),
    }
}

#[test]
fn resurrection_offers_match_native_through_encrypted_packets() -> Result<(), TestError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let (server, mut network) = WorldServer::connect().await?;
            let mut count = 0;
            for line in include_str!("../fixtures/player_resurrection_offer_native.txt")
                .lines()
                .filter(|line| line.starts_with("offer "))
            {
                let r = line.split_whitespace().collect::<Vec<_>>();
                let body = unhex(r[5])?;
                server
                    .exchange(vec![(0x15b, body.clone())], 0)
                    .await?
                    .await??;
                let update = network
                    .receive_packet()
                    .await?
                    .player_resurrection()?
                    .ok_or("offer")?;
                let mut world = ActiveWorld::enter(WorldBootstrap::new(
                    WorldMapId::new(0),
                    7,
                    "Offer",
                    glam::Vec3::ZERO,
                    0.0,
                ));
                let fields = [
                    (24, u32::from_str_radix(r[2], 16)?),
                    (32, 100),
                    (150, r[3].parse()?),
                ];
                world.create_object(7, ObjectKind::Player, None, fields)?;
                solarity_systems::project_object_fields(&mut world, 7, fields)?;
                if r[1] == "0" {
                    let local = world.local_player();
                    assert!(world.storage_mut().delete_entity(local));
                }
                let mut state = RuntimePlayerUiState::default();
                let cache = (r[4] != "-")
                    .then(|| unhex(r[4]))
                    .transpose()?
                    .map(String::from_utf8)
                    .transpose()?;
                if let Some(name) = cache {
                    state.receive_player_name(&world, named(0x1234567800000009, &name));
                }
                state.receive_resurrection(&world, update, 1000);
                let RuntimePlayerUiNotification::ResurrectionOffer { offer, name } =
                    state.take_notification().ok_or("notification")?
                else {
                    return Err("wrong notification".into());
                };
                assert_eq!(
                    (offer.guid, offer.sickness, offer.timer),
                    (u64::from_str_radix(r[6], 16)?, r[7].parse()?, r[8].parse()?),
                    "{line}"
                );
                let expected = r[9]
                    .split(',')
                    .find_map(|e| e.strip_prefix("106:"))
                    .map(unhex)
                    .transpose()?
                    .map(String::from_utf8)
                    .transpose()?;
                assert_eq!(name, expected, "{line}");
                assert!(state.take_notification().is_none());
                // Every prefix here loses a required flag or part of its name region.
                let malformed = (0..body.len())
                    .map(|length| (0x15b, body[..length].to_vec()))
                    .collect();
                server.exchange(malformed, 0).await?.await??;
                for _ in 0..body.len() {
                    assert!(
                        network
                            .receive_packet()
                            .await?
                            .player_resurrection()
                            .is_err()
                    );
                }
                state.clear_for_world_leave();
                assert_eq!(
                    state.offer(),
                    solarity_ui::UiPlayerResurrectionOffer::default()
                );
                count += 1;
            }
            assert_eq!(count, 40);
            let (reader, mut writer) = network.split();
            let exchange = server.exchange_raw(Vec::new(), 3).await?;
            writer
                .send_resurrection_response(0x1234567800000009, true)
                .await?;
            writer
                .send_resurrection_response(0x1234567800000009, false)
                .await?;
            writer.send_reclaim_corpse(0x0000f101abcdef12).await?;
            let packets = exchange.await??;
            assert_eq!(
                packets,
                vec![
                    (0x15c, unhex("090000007856341201")?),
                    (0x15c, unhex("090000007856341200")?),
                    (0x1d2, unhex("12efcdab01f10000")?)
                ]
            );
            drop(reader);
            Ok(())
        })
}

#[test]
fn resurrection_name_callbacks_match_native_current_offer_lookup() -> Result<(), TestError> {
    let mut count = 0;
    for line in include_str!("../fixtures/player_resurrection_offer_native.txt")
        .lines()
        .filter(|line| line.starts_with("callback "))
    {
        let r = line.split_whitespace().collect::<Vec<_>>();
        let mut world = ActiveWorld::enter(WorldBootstrap::new(
            WorldMapId::new(0),
            7,
            "Offer",
            glam::Vec3::ZERO,
            0.0,
        ));
        let fields = [
            (24, u32::from_str_radix(r[2], 16)?),
            (32, 100),
            (150, r[3].parse()?),
        ];
        world.create_object(7, ObjectKind::Player, None, fields)?;
        solarity_systems::project_object_fields(&mut world, 7, fields)?;
        if r[1] == "0" {
            let local = world.local_player();
            assert!(world.storage_mut().delete_entity(local));
        }
        let mut state = RuntimePlayerUiState::default();
        state.receive_resurrection(
            &world,
            solarity_network::WorldPlayerResurrection::Offer {
                guid: 0x1234567800000009,
                name: "Initial".into(),
                sickness: 2,
                timer: 255,
            },
            1000,
        );
        state.discard_published_notifications();
        if r[4] != "-" {
            let name = if r[4] == "empty" {
                String::new()
            } else {
                String::from_utf8(unhex(r[4])?)?
            };
            state.names.receive(named(0x1234567800000009, &name));
        }
        state.complete_offer_name(&world);
        let RuntimePlayerUiNotification::ResurrectionOffer { offer, name } =
            state.take_notification().ok_or("callback")?
        else {
            return Err("notification".into());
        };
        assert_eq!(
            (offer.guid, offer.sickness, offer.timer),
            (u64::from_str_radix(r[5], 16)?, r[6].parse()?, r[7].parse()?),
            "{line}"
        );
        let expected = r[8]
            .split(',')
            .find_map(|e| e.strip_prefix("106:"))
            .map(unhex)
            .transpose()?
            .map(String::from_utf8)
            .transpose()?;
        assert_eq!(name, expected, "{line}");
        count += 1;
    }
    assert_eq!(count, 24);
    Ok(())
}

#[test]
fn resurrection_source_name_packets_match_original_reader_and_query_writer() -> Result<(), TestError>
{
    use solarity_network::WorldPlayerNameResult;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let (server, mut network) = WorldServer::connect().await?;
            let mut count = 0;
            for line in include_str!("../fixtures/player_name_query_native.txt")
                .lines()
                .filter(|line| line.starts_with("response "))
            {
                let row = line.split_whitespace().collect::<Vec<_>>();
                let body = unhex(row[1])?;
                assert_eq!(body.len(), row[2].parse::<usize>()?);
                server
                    .exchange(vec![(0x51, body.clone())], 0)
                    .await?
                    .await??;
                let response = network
                    .receive_packet()
                    .await?
                    .player_name_query()?
                    .ok_or("name query")?;
                let expected = row[3]
                    .split('|')
                    .next()
                    .ok_or("cache write")?
                    .split(':')
                    .collect::<Vec<_>>();
                assert_eq!(
                    response.guid,
                    u64::from_str_radix(expected[1], 16)?,
                    "{line}"
                );
                match response.result {
                    WorldPlayerNameResult::Found(value) => {
                        assert_eq!(expected[0], "found");
                        let text = |s| -> Result<String, TestError> {
                            Ok(if s == "-" {
                                String::new()
                            } else {
                                String::from_utf8(unhex(s)?)?
                            })
                        };
                        assert_eq!(value.name, text(expected[2])?);
                        assert_eq!(value.realm, text(expected[3])?);
                        assert_eq!(
                            (value.race, value.gender, value.class),
                            (
                                expected[4].parse()?,
                                expected[5].parse()?,
                                expected[6].parse()?
                            )
                        );
                        let forms = if expected[7] == "-" {
                            None
                        } else {
                            Some(
                                expected[7]
                                    .split(',')
                                    .map(text)
                                    .collect::<Result<Vec<_>, _>>()?,
                            )
                        };
                        assert_eq!(value.declined.map(Vec::from), forms);
                        assert_eq!(value.flagged, row[3].contains("|flag:"));
                    }
                    WorldPlayerNameResult::Missing => assert_eq!(expected[0], "missing"),
                    WorldPlayerNameResult::Retry => assert_eq!(expected[0], "retry"),
                }
                server
                    .exchange(
                        (0..body.len())
                            .map(|end| (0x51, body[..end].to_vec()))
                            .collect(),
                        0,
                    )
                    .await?
                    .await??;
                for _ in 0..body.len() {
                    assert!(
                        network.receive_packet().await?.player_name_query().is_err(),
                        "{line}"
                    );
                }
                count += 1;
            }
            assert_eq!(count, 18);
            let (_, mut writer) = network.split();
            for line in include_str!("../fixtures/player_name_query_native.txt")
                .lines()
                .filter(|line| line.starts_with("query "))
            {
                let r = line.split_whitespace().collect::<Vec<_>>();
                let guid = u64::from_str_radix(r[1], 16)?;
                let receive = server.exchange_raw(vec![], 1).await?;
                writer.send_player_name_query(guid).await?;
                let expected = unhex(r[2])?;
                assert_eq!(receive.await??, vec![(0x50, expected[4..].to_vec())]);
            }
            Ok(())
        })
}

#[test]
fn resurrection_name_retries_callbacks_and_consumed_guids_retain_order() -> Result<(), TestError> {
    use solarity_network::{
        WorldPlayerNameResponse, WorldPlayerNameResult, WorldPlayerResurrection,
    };
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Offer",
        glam::Vec3::ZERO,
        0.0,
    ));
    world.create_object(7, ObjectKind::Player, None, [(24, 0)])?;
    solarity_systems::project_object_fields(&mut world, 7, [(24, 0)])?;
    let offer = |guid, name: &str| WorldPlayerResurrection::Offer {
        guid,
        name: name.into(),
        sickness: 1,
        timer: 1,
    };
    let mut state = RuntimePlayerUiState::default();
    for _ in 0..2 {
        state.receive_resurrection(&world, offer(10, ""), 1000);
    }
    assert_eq!(state.names.pending_request(), Some(10));
    state.names.request_admitted();
    assert_eq!(state.names.pending_request(), None);
    state.receive_player_name(
        &world,
        WorldPlayerNameResponse {
            guid: 10,
            result: WorldPlayerNameResult::Retry,
        },
    );
    assert_eq!(state.names.pending_request(), Some(10));
    state.names.request_admitted();
    state.receive_resurrection(&world, offer(11, "New source"), 1001);
    state.discard_published_notifications();
    state.receive_player_name(&world, named(10, "Old source"));
    // Both registered callbacks inspect the current GUID (11), which has no
    // cache record even though its packet supplied a display name.
    for _ in 0..2 {
        assert_eq!(
            state.take_notification(),
            Some(RuntimePlayerUiNotification::ResurrectionOffer {
                offer: Default::default(),
                name: None
            })
        );
    }
    assert_eq!(state.offer(), Default::default());
    state.receive_resurrection(&world, offer(12, ""), 1002);
    state.discard_published_notifications();
    let mut consumed = state.offer();
    consumed.guid = 0;
    state.synchronize_consumed_offer(consumed);
    assert_eq!(state.offer().guid, 0);
    state.receive_player_name(&world, named(12, "Late source"));
    assert_eq!(
        state.take_notification(),
        Some(RuntimePlayerUiNotification::ResurrectionOffer {
            offer: Default::default(),
            name: None
        })
    );
    state.receive_resurrection(&world, offer(13, "Latest"), 1003);
    state.synchronize_consumed_offer(solarity_ui::UiPlayerResurrectionOffer {
        guid: 0,
        sickness: 1,
        timer: 1,
    });
    assert_eq!(
        state.offer().guid,
        13,
        "unpublished offer cannot be consumed by stale UI state"
    );
    Ok(())
}
