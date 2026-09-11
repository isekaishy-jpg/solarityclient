//! Compare the retained owner table against original code and seated callbacks.

use super::*;
use std::error::Error;

#[test]
fn seated_vehicle_owner_table_matches_original_registration_and_completion()
-> Result<(), Box<dyn Error>> {
    let mut checked = 0;
    for line in include_str!("../fixtures/vehicle_animation_owner_native.txt").lines() {
        if line.starts_with('#') {
            continue;
        }
        let row = line
            .split_whitespace()
            .map(str::parse::<i64>)
            .collect::<Result<Vec<_>, _>>()?;
        let [
            key,
            count,
            missing,
            reason,
            controls,
            registered,
            low,
            high,
            action,
            ..,
        ] = row[..]
        else {
            return Err("native owner row".into());
        };
        let mut state = UnitVehicleAnimations {
            owned: u64::MAX,
            ..Default::default()
        };
        let lifetimes = (1..=count)
            .map(|guid| Rc::new(Cell::new(missing == 2 || missing == 1 && guid == 1)))
            .collect::<Vec<_>>();
        for (index, lifetime) in lifetimes.iter().enumerate() {
            state.register(index as u64 + 1, key as i32, lifetime);
        }
        assert_eq!(state.controls(key as i32), controls != 0, "{line}");
        if count != 0 {
            assert_eq!(
                state
                    .owners
                    .iter()
                    .flatten()
                    .any(|owner| owner.guid == count as u64),
                registered != 0,
                "{line}"
            );
        }
        let active = state.finish(key as i32, reason != 0);
        assert_eq!(state.owned, low as u64 | (high as u64) << 32, "{line}");
        let expected_action = if reason != 0 {
            0
        } else if active {
            1
        } else if normalized_key(key as i32) == 26 {
            3
        } else {
            2
        };
        assert_eq!(expected_action, action, "{line}");
        assert_eq!(
            state
                .owners
                .iter()
                .map(|owner| owner.as_ref().map_or(0, |owner| owner.guid as i64))
                .collect::<Vec<_>>(),
            row[9..],
            "{line}"
        );
        checked += 1;
    }
    assert_eq!(checked, 150);
    Ok(())
}
