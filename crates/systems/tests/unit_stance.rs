//! Original executable decisions, independent of the Rust posture implementation.

use solarity_systems::{
    UnitPrimaryAnimationCompletion, UnitStandAnimationDecision,
    resolve_unit_primary_animation_completion, resolve_unit_stand_animation,
    resolve_unit_stand_completion, resolve_unit_stand_transition,
};

#[test]
fn posture_selection_and_completion_match_original_client() -> Result<(), Box<dyn std::error::Error>>
{
    for line in include_str!("fixtures/unit-stance-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let row: Vec<_> = line.split_whitespace().collect();
        if row[0] == "select" {
            let expected = match (row[6], row[7]) {
                ("0", _) => UnitStandAnimationDecision::Continue,
                (_, "-1") => UnitStandAnimationDecision::Retain,
                (_, id) => UnitStandAnimationDecision::Select(id.parse()?),
            };
            assert_eq!(
                resolve_unit_stand_animation(
                    row[1].parse()?,
                    row[2].parse()?,
                    row[3].parse()?,
                    row[4] == "1",
                    row[5] == "1"
                ),
                expected,
                "{line}"
            );
        } else if row[0] == "entry" {
            let expected = match row[6] {
                "-1" => UnitStandAnimationDecision::Continue,
                "-2" => UnitStandAnimationDecision::Retain,
                id => UnitStandAnimationDecision::Select(id.parse()?),
            };
            assert_eq!(
                resolve_unit_stand_transition(
                    row[1].parse()?,
                    row[2].parse()?,
                    row[3].parse()?,
                    row[4] == "1",
                    row[5] == "1"
                ),
                expected,
                "{line}"
            );
        } else if row[0] == "death" {
            let expected = match (row[4], row[5]) {
                ("-1", _) => UnitPrimaryAnimationCompletion::Continue,
                ("-2", _) => UnitPrimaryAnimationCompletion::Retain,
                (id, "1") => {
                    UnitPrimaryAnimationCompletion::SelectWithCurrentVariation(id.parse()?)
                }
                (id, _) => UnitPrimaryAnimationCompletion::Select(id.parse()?),
            };
            assert_eq!(
                resolve_unit_primary_animation_completion(
                    row[1].parse()?,
                    0,
                    row[2] == "1",
                    row[3] == "1"
                ),
                expected,
                "{line}"
            );
        } else {
            let expected = if row[3] == "-1" {
                None
            } else {
                Some(row[3].parse()?)
            };
            assert_eq!(
                resolve_unit_stand_completion(row[2].parse()?, row[1].parse()?),
                expected,
                "{line}"
            );
        }
    }
    Ok(())
}
