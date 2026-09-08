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
