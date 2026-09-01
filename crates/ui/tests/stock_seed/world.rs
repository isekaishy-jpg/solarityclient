//! External stock-compatibility tests for active-world FrameXML state.

use solarity_ui::{
    UiFactionGroup, UiPlayerFactionState, UiPlayerProgressionState, UiPlayerState, UiWorldState,
    UiZonePvpType, UiZoneState,
};

/// Player entry, live replacement, and world exit preserve explicit absence.
#[test]
fn world_state_retains_only_authoritative_player_facts() {
    let world = UiWorldState::new();
    assert_eq!(world.player(), None);
    assert_eq!(world.player_progression(), None);
    assert_eq!(world.player_faction(), None);
    assert_eq!(world.zone(), None);

    world.enter_player(UiPlayerState::new(12_345_678));
    world.set_player_progression(UiPlayerProgressionState::new(123_456, 1_000_000));
    world.set_player_faction(UiPlayerFactionState::new(
        UiFactionGroup::Alliance,
        "Alliance",
    ));
    assert_eq!(
        world.player().map(UiPlayerState::money_copper),
        Some(12_345_678)
    );
    assert_eq!(
        world.player_progression(),
        Some(UiPlayerProgressionState::new(123_456, 1_000_000))
    );
    assert_eq!(
        world.player_faction(),
        Some(UiPlayerFactionState::new(
            UiFactionGroup::Alliance,
            "Alliance"
        ))
    );
    assert_eq!(world.cursor_money_copper(), 0);
    assert_eq!(world.player_trade_money_copper(), 0);

    world.set_cursor_money_copper(234);
    world.set_player_trade_money_copper(567);
    world.set_zone(UiZoneState::new(
        "Elwynn Forest",
        "Northshire Valley",
        Some(UiZonePvpType::Friendly),
        true,
        Some("Alliance".to_owned()),
    ));
    assert_eq!(world.cursor_money_copper(), 234);
    assert_eq!(world.player_trade_money_copper(), 567);
    assert_eq!(
        world.zone(),
        Some(UiZoneState::new(
            "Elwynn Forest",
            "Northshire Valley",
            Some(UiZonePvpType::Friendly),
            true,
            Some("Alliance".to_owned()),
        ))
    );

    world.enter_player(UiPlayerState::new(u32::MAX));
    assert_eq!(
        world.player().map(UiPlayerState::money_copper),
        Some(u32::MAX)
    );

    world.leave_world();
    assert_eq!(world.player(), None);
    assert_eq!(world.player_progression(), None);
    assert_eq!(world.player_faction(), None);
    assert_eq!(world.zone(), None);
    assert_eq!(world.cursor_money_copper(), 0);
    assert_eq!(world.player_trade_money_copper(), 0);
}
