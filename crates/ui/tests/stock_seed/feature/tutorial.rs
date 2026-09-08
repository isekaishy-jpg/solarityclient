//! Captured tutorial-owner state and native Lua return conventions.

use super::{UiTutorialAction, UiTutorialState};

#[test]
fn tutorial_state_matches_original_flags_history_and_discovery_order()
-> Result<(), Box<dyn std::error::Error>> {
    let state = UiTutorialState::default();
    let fixture = include_str!("../../fixtures/tutorial_state_native.txt");
    let mut count = 0;
    for line in fixture.lines() {
        let row = line
            .split_ascii_whitespace()
            .map(str::parse::<u32>)
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(row.len(), 93);
        let mut result = u32::MAX;
        let mut event = u32::MAX;
        let mut sound = 0;
        let mut before = [0; 8];
        match row[0] {
            0 => match row[1] {
                0 => state.replace_flags(&[0; 32]),
                1 => {
                    let mut bytes = [0; 32];
                    bytes[..4].copy_from_slice(&(1_u32 << 26).to_le_bytes());
                    state.replace_flags(&bytes);
                }
                2 => state.replace_flags(&[]),
                _ => panic!("native receive case"),
            },
            1 => {
                if state.needs_trigger(row[1]) {
                    if row[2] != 0 {
                        event = row[1] + 1;
                        sound = 1;
                        for (index, value) in before.iter_mut().enumerate() {
                            *value = state
                                .inner
                                .borrow()
                                .seen
                                .words
                                .get(index)
                                .copied()
                                .unwrap_or(0);
                        }
                        state.mark_triggered(row[1]);
                    } else {
                        state.mark_triggered(row[1]);
                        state.flag(row[1]);
                    }
                }
            }
            2 => state.flag(row[1]),
            3 => state.clear(),
            4 => state.reset(),
            5 => result = state.next_completed(row[1]).unwrap_or(60),
            6 => result = state.previous_completed(row[1]).unwrap_or(0),
            7 => result = u32::from(state.is_flagged(row[1])),
            8 => result = u32::from(state.can_reset()),
            _ => panic!("native operation"),
        }
        assert_eq!(
            (result, event, sound),
            (row[4], row[7], row[8]),
            "operation {} argument {} case {count}",
            row[0],
            row[1]
        );
        let packet = match state.pending_action() {
            Some(UiTutorialAction::Flag(index)) => [0xfe, index],
            Some(UiTutorialAction::Clear) => [0xff, u32::MAX],
            Some(UiTutorialAction::Reset) => [0x100, u32::MAX],
            None => [u32::MAX; 2],
        };
        assert_eq!(packet, row[5..7], "packet case {count}");
        state.accept_action();
        assert_eq!(state.pending_action(), None);
        let inner = state.inner.borrow();
        assert_eq!(inner.seen.bit_count as u32, row[3]);
        for index in 0..8 {
            assert_eq!(
                inner.seen.words.get(index).copied().unwrap_or(0),
                row[9 + index],
                "seen case {count}"
            );
            assert_eq!(
                inner.completed.words.get(index).copied().unwrap_or(0),
                row[17 + index],
                "completed case {count}"
            );
        }
        assert_eq!(inner.history, row[25..85], "history case {count}");
        assert_eq!(
            before,
            row[85..93],
            "event observes preceding discovery flags case {count}"
        );
        count += 1;
    }
    assert_eq!(count, 122);
    Ok(())
}

#[test]
fn tutorial_lua_retains_numeric_coercion_counts_and_native_usage_messages() -> mlua::Result<()> {
    let lua = mlua::Lua::new();
    let state = UiTutorialState::default();
    state.replace_flags(&[0; 32]);
    super::register_globals(&lua, &lua.globals(), state.clone())?;
    lua.load(r#"
assert(select('#', CanResetTutorials()) == 1 and CanResetTutorials() == nil)
assert(select('#', IsTutorialFlagged(28)) == 1 and IsTutorialFlagged(28) == nil)
assert(select('#', IsTutorialFlagged(0)) == 0)
assert(select('#', IsTutorialFlagged(61)) == 0)
assert(select('#', GetNextCompleatedTutorial(28)) == 0)
assert(select('#', GetPrevCompleatedTutorial(28)) == 0)
assert(select('#', FlagTutorial('28.9')) == 0)
assert(IsTutorialFlagged('28') == 1 and CanResetTutorials() == 1)
FlagTutorial(29); FlagTutorial(60)
assert(GetNextCompleatedTutorial(28) == 29)
assert(select('#', GetNextCompleatedTutorial(29)) == 0)
assert(GetPrevCompleatedTutorial(60) == 29)
assert(GetPrevCompleatedTutorial(256) == 60)
local ok,err=pcall(IsTutorialFlagged,'invalid'); assert(not ok and string.find(tostring(err),'Usage: Trigger("tutorial")',1,true))
for _,func in ipairs({FlagTutorial,IsTutorialFlagged,GetNextCompleatedTutorial,GetPrevCompleatedTutorial}) do
 assert(not pcall(func)); assert(not pcall(func,{})); assert(not pcall(func,true))
end
assert(select('#', ClearTutorials()) == 0); assert(IsTutorialFlagged(1) == 1)
assert(select('#', ResetTutorials()) == 0); assert(IsTutorialFlagged(28) == nil)
assert(CanResetTutorials() == nil and select('#', GetPrevCompleatedTutorial(29)) == 0)
"#).exec()?;
    let mut actions = Vec::new();
    while let Some(action) = state.pending_action() {
        actions.push(action);
        state.accept_action();
    }
    assert_eq!(
        actions,
        vec![
            UiTutorialAction::Flag(27),
            UiTutorialAction::Flag(28),
            UiTutorialAction::Flag(59),
            UiTutorialAction::Clear,
            UiTutorialAction::Reset
        ]
    );
    Ok(())
}
