//! Decisions captured from the original executable, independent of Rust rules.

use solarity_systems::{
    UnitMovementAnimationDecision, resolve_unit_airborne_animation, resolve_unit_landing_animation,
    resolve_unit_movement_animation_completion, resolve_unit_turn_animation,
    unit_movement_is_airborne,
};

fn decision(value: &str) -> Result<UnitMovementAnimationDecision, std::num::ParseIntError> {
    Ok(match value {
        "-1" => UnitMovementAnimationDecision::Continue,
        "-2" => UnitMovementAnimationDecision::Retain,
        value => UnitMovementAnimationDecision::Select(value.parse()?),
    })
}

#[test]
fn movement_animation_matches_original_client() -> Result<(), Box<dyn std::error::Error>> {
    for line in include_str!("fixtures/unit-movement-animation-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let row: Vec<_> = line.split_whitespace().collect();
        match row[0] {
            "land" => assert_eq!(
                resolve_unit_landing_animation(
                    row[1].parse()?,
                    row[2].parse()?,
                    row[3] == "1",
                    row[4] == "1"
                ),
                decision(row[5])?,
                "{line}"
            ),
            "turn" => assert_eq!(
                resolve_unit_turn_animation(
                    row[1].parse()?,
                    row[2].parse()?,
                    row[3].parse()?,
                    row[4] != "0"
                ),
                if row[5] == "-1" {
                    None
                } else {
                    Some(row[5].parse()?)
                },
                "{line}"
            ),
            "fall" => assert_eq!(
                unit_movement_is_airborne(row[1].parse()?, row[2].parse()?),
                row[3] == "1",
                "{line}"
            ),
            "complete" => assert_eq!(
                resolve_unit_movement_animation_completion(
                    row[1].parse()?,
                    u32::from(row[2] == "1") * 0x2000000
                ),
                decision(row[3])?,
                "{line}"
            ),
            "family" => assert_eq!(
                resolve_unit_airborne_animation(row[1].parse()?),
                if row[2] == "1" {
                    UnitMovementAnimationDecision::Retain
                } else {
                    UnitMovementAnimationDecision::Select(40)
                },
                "{line}"
            ),
            _ => panic!("unknown native fixture row: {line}"),
        }
    }
    Ok(())
}
