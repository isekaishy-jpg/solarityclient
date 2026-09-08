use super::*;

thread_local! { static NOW: std::cell::Cell<u32> = const { std::cell::Cell::new(0) }; }
fn now() -> u32 {
    NOW.get()
}

#[test]
fn release_timer_initialization_and_lua_countdown_match_native()
-> Result<(), Box<dyn std::error::Error>> {
    let lua = mlua::Lua::new();
    let environment = crate::UiScriptEnvironment::new(800, 600, false)?
        .with_client_clock(crate::UiClientClock::from_source(now));
    let world = environment.world_state();
    register_globals(&lua, &lua.globals(), &environment)?;
    let query: mlua::Function = lua.globals().get("GetReleaseTimeRemaining")?;
    assert_eq!(query.call::<i32>(())?, 0);
    let mut cases = 0;
    for line in include_str!("../fixtures/player_release_native.txt").lines() {
        let words = line.split_whitespace().collect::<Vec<_>>();
        if words.first() != Some(&"release") {
            continue;
        }
        let hex = |i| u32::from_str_radix(words[i], 16);
        let timer = UiPlayerReleaseTimer::on_death(hex(1)? as u8, hex(2)?, hex(3)?);
        assert_eq!(timer.deadline_ms, hex(5)?, "{line}");
        assert_eq!(timer.no_timer, words[6] == "1", "{line}");
        world.set_release_timer(timer);
        NOW.set(hex(4)?);
        assert_eq!(query.call::<i32>(())?, words[7].parse::<i32>()?, "{line}");
        cases += 1;
    }
    assert_eq!(cases, 216);
    Ok(())
}

#[test]
fn death_dialog_queries_and_repop_admission_match_original_player_vtable()
-> Result<(), Box<dyn std::error::Error>> {
    let lua = mlua::Lua::new();
    let environment = crate::UiScriptEnvironment::new(800, 600, false)?;
    let world = environment.world_state();
    let battlefield = environment.battlefield_state();
    register_globals(&lua, &lua.globals(), &environment)?;
    crate::feature::register_battlefield_globals(&lua, &lua.globals(), battlefield.clone())?;
    let query = |name: &str| lua.globals().get::<mlua::Function>(name);
    let mut cases = 0;
    for line in include_str!("../fixtures/player_death_dialog_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let row: Vec<_> = line.split_ascii_whitespace().collect();
        let hex = |i| u32::from_str_radix(row[i], 16);
        world.leave_world();
        if row[1] == "1" {
            world.enter_player(crate::UiPlayerState::new(0));
            world.set_player_vitals(crate::UiPlayerVitalsState::new(
                100,
                100,
                0,
                0,
                crate::UiUnitPowerType::Mana,
            ));
        }
        match row[0] {
            "cinematic" => {
                world.set_in_cinematic(hex(1)? != 0);
                assert_eq!(
                    query("InCinematic")?.call::<Option<u32>>(())?,
                    (row[2] == "1").then_some(1),
                    "{line}"
                );
            }
            "selfres" => {
                if let Some(vitals) = world.player_vitals() {
                    world.set_player_vitals(vitals.with_health(hex(2)?, 100, 12345, false));
                }
                world.set_resurrection_state(UiPlayerResurrectionState {
                    self_resurrection_spell: row[3].parse()?,
                    blocked: row[4] == "0",
                    ..Default::default()
                });
                query("UseSoulstone")?.call::<()>(())?;
                assert_eq!(
                    world.pending_death_action(),
                    (row[5] != "none").then_some(UiPlayerDeathAction::SelfResurrect),
                    "{line}"
                );
                if row[5] != "none" {
                    assert_eq!(row[5], "2b3");
                }
                world.accept_death_action();
            }
            "repop" => {
                if let Some(vitals) = world.player_vitals() {
                    world.set_player_vitals(vitals.with_health(
                        hex(2)?,
                        100,
                        12345,
                        hex(3)? & 0x10 != 0,
                    ));
                }
                world.set_resurrection_state(UiPlayerResurrectionState {
                    blocked: row[4] == "0",
                    ..Default::default()
                });
                query("RepopMe")?.call::<()>(())?;
                assert_eq!(
                    world.pending_death_action().is_some(),
                    row[5] != "none",
                    "{line}"
                );
                if row[5] != "none" {
                    assert_eq!(row[5], "15a,0");
                }
                world.accept_death_action();
                assert!(!world.pending_death_action().is_some(), "{line}");
            }
            "falling" | "bounds" | "blocked" => {
                let value = hex(2)?;
                let name = match row[0] {
                    "falling" => {
                        world.set_falling(value & 0x1800 == 0x1000);
                        "IsFalling"
                    }
                    "bounds" => {
                        world.set_resurrection_state(UiPlayerResurrectionState {
                            out_of_bounds: value & 0x4000 != 0,
                            ..Default::default()
                        });
                        "IsOutOfBounds"
                    }
                    _ => {
                        world.set_resurrection_state(UiPlayerResurrectionState {
                            blocked: value & 4 != 0,
                            ..Default::default()
                        });
                        "CannotBeResurrected"
                    }
                };
                assert_eq!(
                    query(name)?.call::<Option<u32>>(())?,
                    (row[3] == "1").then_some(1),
                    "{line}"
                );
            }
            "soulstone" => {
                if let Some(vitals) = world.player_vitals() {
                    world.set_player_vitals(vitals.with_health(hex(2)?, 100, 12345, false));
                }
                let name = if row[3] != "0" {
                    Some(if row[4] == "1" {
                        "Self resurrection"
                    } else {
                        "UNKNOWN"
                    })
                } else if row[5] == "1" {
                    Some("Inventory resurrection")
                } else {
                    None
                };
                world.set_resurrection_state(UiPlayerResurrectionState {
                    self_resurrection_name: name.map(str::to_owned),
                    ..Default::default()
                });
                let result = query("HasSoulstone")?.call::<Option<String>>(())?;
                let expected = match row[6] {
                    "nil" => None,
                    "empty" => Some(String::new()),
                    text => Some(text.replace('_', " ")),
                };
                assert_eq!(result, expected, "{line}");
            }
            "arena" => {
                for index in 1..=2 {
                    battlefield.set_slot(
                        index,
                        crate::UiBattlefieldSlot::active(
                            crate::UiBattlefieldQueueStatus::Active,
                            "Arena",
                            1,
                            (1, 80),
                            2,
                            row[3] == "1",
                        )?,
                    )?;
                }
                battlefield.set_active_battlefield(row[1].parse()?, Some(hex(2)? as usize));
                let result =
                    query("IsActiveBattlefieldArena")?.call::<(Option<u32>, Option<u32>)>(())?;
                let expected: Vec<_> = row[4]
                    .split(',')
                    .map(|value| (value == "1").then_some(1))
                    .collect();
                assert_eq!(result, (expected[0], expected[1]), "{line}");
            }
            _ => panic!("unexpected oracle row {line}"),
        }
        cases += 1;
    }
    assert_eq!(cases, 190);
    Ok(())
}
