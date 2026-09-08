use super::*;

thread_local! { static NOW: std::cell::Cell<u32> = const { std::cell::Cell::new(0) }; }
fn now() -> u32 {
    NOW.get()
}

#[test]
fn resurrection_actions_flags_and_recovery_match_original_lua()
-> Result<(), Box<dyn std::error::Error>> {
    let lua = mlua::Lua::new();
    let environment = crate::UiScriptEnvironment::new(800, 600, false)?
        .with_client_clock(crate::UiClientClock::from_source(now));
    let world = environment.world_state();
    register_globals(&lua, &lua.globals(), &environment)?;
    super::super::player_release::register_globals(&lua, &lua.globals(), &environment)?;
    let mut count = 0;
    for line in include_str!("../../../runtime/tests/fixtures/player_resurrection_offer_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let r = line.split_whitespace().collect::<Vec<_>>();
        let query = |name: &str| lua.globals().get::<mlua::Function>(name);
        match r[0] {
            "action" => {
                world.leave_world();
                if r[1] == "1" {
                    world.enter_player(crate::UiPlayerState::new(0));
                }
                world.set_resurrection_offer(UiPlayerResurrectionOffer {
                    guid: u64::from_str_radix(r[2], 16)?,
                    sickness: 2,
                    timer: 255,
                });
                world.set_corpse_state(UiPlayerCorpseState {
                    guid: 0x0000f101abcdef12,
                    ..Default::default()
                });
                let action = match r[3] {
                    "accept" => "AcceptResurrect",
                    "decline" => "DeclineResurrect",
                    "reclaim" => "RetrieveCorpse",
                    _ => return Err("action".into()),
                };
                query(action)?.call::<()>(())?;
                let offer = world.resurrection_offer();
                assert_eq!(
                    (offer.guid, offer.sickness, offer.timer),
                    (u64::from_str_radix(r[4], 16)?, r[5].parse()?, r[6].parse()?),
                    "{line}"
                );
                let mut bytes = String::new();
                while let Some(action) = world.pending_death_action() {
                    let mut packet = Vec::new();
                    match action {
                        super::super::UiPlayerDeathAction::ResurrectionResponse {
                            guid,
                            accept,
                        } => {
                            packet.extend(0x15c_u32.to_le_bytes());
                            packet.extend(guid.to_le_bytes());
                            packet.push(u8::from(accept));
                        }
                        super::super::UiPlayerDeathAction::ReclaimCorpse { guid } => {
                            packet.extend(0x1d2_u32.to_le_bytes());
                            packet.extend(guid.to_le_bytes());
                        }
                        _ => return Err("unexpected death action".into()),
                    }
                    for byte in packet {
                        use std::fmt::Write;
                        write!(bytes, "{byte:02x}")?;
                    }
                    bytes.push('/');
                    world.accept_death_action();
                }
                assert_eq!(if bytes.is_empty() { "-" } else { &bytes }, r[7], "{line}");
                // A second click cannot emit another response after consumption.
                if r[3] != "reclaim" {
                    query(action)?.call::<()>(())?;
                    assert!(world.pending_death_action().is_none());
                }
            }
            "query" => {
                world.set_resurrection_offer(UiPlayerResurrectionOffer {
                    sickness: r[1].parse()?,
                    timer: r[2].parse()?,
                    ..Default::default()
                });
                environment
                    .battlefield_state()
                    .set_active_battlefield(r[3].parse()?, None);
                for (api, expected) in [("ResurrectHasSickness", r[4]), ("ResurrectHasTimer", r[5])]
                {
                    assert_eq!(
                        query(api)?.call::<Option<u32>>(())?,
                        (expected == "1").then_some(1),
                        "{line}"
                    );
                }
            }
            "delay" => {
                NOW.set(r[1].parse()?);
                let mut corpse = UiPlayerCorpseState {
                    in_range: r[3] == "1",
                    maps: (r[4].parse()?, r[5].parse()?),
                    ..Default::default()
                };
                corpse.set_delay(r[2].parse()?, NOW.get());
                assert_eq!(corpse.deadline_ms, r[6].parse()?, "{line}");
                world.set_corpse_state(corpse);
                assert_eq!(
                    query("GetCorpseRecoveryDelay")?.call::<u32>(())?,
                    r[7].parse()?,
                    "{line}"
                );
                assert_eq!(
                    corpse.range_event(),
                    match r[8] {
                        "184" => Some("CORPSE_IN_RANGE"),
                        "185" => Some("CORPSE_IN_INSTANCE"),
                        "-" => None,
                        _ => return Err("event".into()),
                    }
                );
            }
            "controlling" => {
                world.leave_world();
                if r[1] == "1" {
                    world.enter_player(crate::UiPlayerState::new(0));
                }
                world.set_resurrection_state(crate::UiPlayerResurrectionState {
                    controlling: u64::from_str_radix(r[3], 16)? != 0
                        || u64::from_str_radix(r[4], 16)? != 0,
                    ..Default::default()
                });
                assert_eq!(
                    query("UnitIsControlling")?.call::<Option<u32>>(if r[2] == "1" {
                        "player"
                    } else {
                        "target"
                    })?,
                    (r[5] == "1").then_some(1),
                    "{line}"
                );
            }
            "offer" | "callback" => continue,
            _ => return Err("row".into()),
        }
        count += 1;
    }
    assert_eq!(count, 157);
    world.leave_world();
    assert_eq!(
        world.resurrection_offer(),
        UiPlayerResurrectionOffer::default()
    );
    assert_eq!(query_zero_delay(&lua)?, 0);
    Ok(())
}

fn query_zero_delay(lua: &mlua::Lua) -> mlua::Result<u32> {
    lua.globals()
        .get::<mlua::Function>("GetCorpseRecoveryDelay")?
        .call(())
}
