//! Raw outputs captured by the original ordinary body controller.
use super::*;

#[test]
fn body_orientation_matches_original_client() -> Result<(), Box<dyn std::error::Error>> {
    verify(
        include_str!("../fixtures/unit-body-orientation-native.txt"),
        false,
        1176,
    )
}

#[test]
fn body_orientation_retains_original_state_across_direction_and_turn_changes()
-> Result<(), Box<dyn std::error::Error>> {
    verify(
        include_str!("../fixtures/unit-body-orientation-timeline-native.txt"),
        true,
        1680,
    )
}

fn verify(
    fixture: &str,
    retained: bool,
    expected_count: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut cases = 0;
    let mut body = UnitBodyOrientation::new(0.);
    for line in fixture.lines() {
        if line.starts_with('#') {
            if line == "# reset" {
                body = UnitBodyOrientation::new(0.);
            }
            continue;
        }
        let words: Vec<_> = line.split_whitespace().collect();
        let decimal = |i: usize| words[i].parse::<u32>();
        let hex = |i: usize| u32::from_str_radix(words[i], 16);
        let float = |i: usize| hex(i).map(f32::from_bits);
        if !retained {
            body = UnitBodyOrientation {
                yaw: float(8)?,
                velocity: float(9)?,
                smoothing: float(10)?,
                spine: None,
                head: None,
            };
        }
        let input = UnitBodyOrientationInput {
            facing: float(11)?,
            movement_flags: decimal(0)?,
            turn_rate: float(13)?,
            facing_elapsed_ms: decimal(7)?,
            frame_seconds: float(12)?,
            direct_facing: decimal(2)? != 0,
            full_spine_turn: decimal(3)? != 0 && decimal(6)? == 0,
            has_spine: decimal(1)? & 0x80 != 0,
            has_head: decimal(1)? & 0x100 != 0,
            mounted: decimal(4)? != 0,
            body_yaw_allowed: decimal(5)? != 0,
        };
        let sample = body.advance(input);
        let actual = [
            body.yaw.to_bits(),
            body.velocity.to_bits(),
            body.smoothing.to_bits(),
            if sample.spine.is_some() { 0x80 } else { 0 },
            sample.spine.unwrap_or(0.).to_bits(),
            if sample.head.is_some() { 0x80 } else { 0 },
            sample.head.unwrap_or(0.).to_bits(),
            sample.procedural_turn,
        ];
        let expected = (14..22).map(hex).collect::<Result<Vec<_>, _>>()?;
        assert_eq!(actual.as_slice(), expected, "{line}");
        let resolver_admitted = input.movement_flags & 0x2e0100f == 0
            && crate::resolve_unit_turn_animation(
                input.movement_flags,
                0,
                sample.procedural_turn,
                false,
            )
            .is_some();
        assert_eq!(
            u32::from(resolver_admitted),
            hex(22)?,
            "native primary-zero resolver gate: {line}"
        );
        cases += 1;
    }
    assert_eq!(cases, expected_count);
    Ok(())
}
