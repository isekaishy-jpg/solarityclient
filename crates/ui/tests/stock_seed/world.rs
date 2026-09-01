//! External stock-compatibility tests for active-world FrameXML state.

use std::error::Error;

use solarity_ui::{
    UiFactionGroup, UiPlayerFactionState, UiPlayerProgressionState, UiPlayerState, UiRealmDate,
    UiRealmDateError, UiRealmTime, UiRealmTimeError, UiWorldState, UiZonePvpType, UiZoneState,
};

/// Player entry, live replacement, and world exit preserve explicit absence.
#[test]
fn world_state_retains_only_authoritative_player_facts() -> Result<(), Box<dyn Error>> {
    let world = UiWorldState::new();
    assert_eq!(world.player(), None);
    assert_eq!(world.player_progression(), None);
    assert_eq!(world.player_faction(), None);
    assert_eq!(world.zone(), None);
    assert_eq!(world.realm_date(), None);
    assert_eq!(world.realm_time(), None);

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
        "Elwynn Forest",
        "Northshire Valley",
        "Northshire Valley",
        Some(UiZonePvpType::Friendly),
        true,
        Some("Alliance".to_owned()),
    ));
    world.set_realm_time(UiRealmTime::new(21, 37)?);
    world.set_realm_date(UiRealmDate::new(3, 12, 8, 2009)?);
    assert_eq!(world.cursor_money_copper(), 234);
    assert_eq!(world.player_trade_money_copper(), 567);
    let zone = match world.zone() {
        Some(zone) => zone,
        None => panic!("zone was just published"),
    };
    assert_eq!(zone.zone_text(), "Elwynn Forest");
    assert_eq!(zone.real_zone_text(), "Elwynn Forest");
    assert_eq!(zone.sub_zone_text(), "Northshire Valley");
    assert_eq!(zone.minimap_zone_text(), "Northshire Valley");
    assert_eq!(world.realm_time(), Some(UiRealmTime::new(21, 37)?));
    assert_eq!(world.realm_date(), Some(UiRealmDate::new(3, 12, 8, 2009)?));
    assert_eq!(
        world.zone(),
        Some(UiZoneState::new(
            "Elwynn Forest",
            "Elwynn Forest",
            "Northshire Valley",
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
    assert_eq!(world.realm_date(), None);
    assert_eq!(world.realm_time(), None);
    assert_eq!(world.cursor_money_copper(), 0);
    assert_eq!(world.player_trade_money_copper(), 0);
    Ok(())
}

/// Realm dates preserve Lua indexing and reject impossible Gregorian values.
#[test]
fn realm_date_preserves_stock_calendar_domains() -> Result<(), Box<dyn Error>> {
    let date = UiRealmDate::new(3, 12, 8, 2009)?;
    assert_eq!(date.weekday(), 3);
    assert_eq!(date.month(), 12);
    assert_eq!(date.month_day(), 8);
    assert_eq!(date.year(), 2009);
    assert_eq!(
        UiRealmDate::new(0, 1, 1, 2009),
        Err(UiRealmDateError::Weekday { weekday: 0 })
    );
    assert_eq!(
        UiRealmDate::new(1, 13, 1, 2009),
        Err(UiRealmDateError::Month { month: 13 })
    );
    assert_eq!(
        UiRealmDate::new(1, 1, 1, 0),
        Err(UiRealmDateError::Year { year: 0 })
    );
    assert!(matches!(
        UiRealmDate::new(1, 2, 29, 2009),
        Err(UiRealmDateError::MonthDay { .. })
    ));
    assert!(UiRealmDate::new(1, 2, 29, 2008).is_ok());
    Ok(())
}

/// Realm time rejects values the native 24-hour clock cannot represent.
#[test]
fn realm_time_preserves_stock_hour_and_minute_domains() -> Result<(), Box<dyn Error>> {
    let time = UiRealmTime::new(23, 59)?;
    assert_eq!(time.hour(), 23);
    assert_eq!(time.minute(), 59);
    assert_eq!(
        UiRealmTime::new(24, 0),
        Err(UiRealmTimeError::Hour { hour: 24 })
    );
    assert_eq!(
        UiRealmTime::new(0, 60),
        Err(UiRealmTimeError::Minute { minute: 60 })
    );
    Ok(())
}
