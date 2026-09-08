//! Original-instruction fixtures for unit CEffect decisions.

use glam::Vec3;
use solarity_systems::{
    UnitBreathEnvironment, UnitBreathState, UnitEffectScale, UnitWaterEffect, UnitWaterSprayInput,
    unit_world_effect_factor,
};

fn float(value: &str) -> f32 {
    match u32::from_str_radix(value, 16) {
        Ok(bits) => f32::from_bits(bits),
        Err(error) => panic!("invalid fixture float {value}: {error}"),
    }
}

fn word(value: &str) -> u32 {
    integer(value)
}

fn integer<T: std::str::FromStr>(value: &str) -> T
where
    T::Err: std::fmt::Display,
{
    match value.parse() {
        Ok(number) => number,
        Err(error) => panic!("invalid fixture integer {value}: {error}"),
    }
}

fn slot(value: Option<UnitWaterEffect>) -> i32 {
    value.map_or(-1, |effect| effect as i32)
}

#[test]
fn unit_water_effect_decisions_match_original_instructions() {
    let mut counts = [0; 4];
    for line in include_str!("../fixtures/unit-water-effects-native.txt").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let values: Vec<_> = line.split_ascii_whitespace().collect();
        match values[0] {
            "B" => {
                let environment = UnitBreathEnvironment {
                    model_height: float(values[1]),
                    unit_scale: float(values[2]),
                    origin_z: float(values[3]),
                    liquid_surface: (word(values[5]) != 0).then(|| float(values[4])),
                    cold_area: word(values[6]) != 0,
                };
                let mut state = UnitBreathState::default();
                state.refresh(word(values[7]), environment);
                let native_bits = word(values[8]) & 0x60;
                let expected = match native_bits {
                    0x20 => 2,
                    0x40 => 3,
                    0 => -1,
                    _ => panic!("unexpected breath flags"),
                };
                assert_eq!(slot(state.effect(0, Some(0), None)), expected, "{line}");
                let deadline = word(values[9]);
                assert!(!state.refresh_due(deadline.wrapping_sub(1)), "{line}");
                assert!(state.refresh_due(deadline), "{line}");
                assert!(state.refresh_due(deadline.wrapping_add(1)), "{line}");
                counts[0] += 1;
            }
            "E" => {
                let native_bits = word(values[1]);
                let mut state = UnitBreathState::default();
                state.refresh(
                    0,
                    UnitBreathEnvironment {
                        model_height: 2.0,
                        unit_scale: 1.0,
                        origin_z: 0.0,
                        liquid_surface: (native_bits & 0x20 != 0).then_some(8.0),
                        cold_area: native_bits & 0x40 != 0,
                    },
                );
                let flags = if values[3] == "-1" {
                    None
                } else {
                    Some(word(values[3]))
                };
                let drunk = (word(values[4]) != 0).then(|| f64::from(float(values[5])));
                assert_eq!(
                    slot(state.effect(word(values[2]), flags, drunk)),
                    integer::<i32>(values[6]),
                    "{line}"
                );
                counts[1] += 1;
            }
            "I" => {
                let native_bits = word(values[1]);
                let mut state = UnitBreathState::default();
                state.refresh(
                    0,
                    UnitBreathEnvironment {
                        model_height: 2.0,
                        unit_scale: 1.0,
                        origin_z: 0.0,
                        liquid_surface: (native_bits & 0x20 != 0).then_some(8.0),
                        cold_area: native_bits & 0x40 != 0,
                    },
                );
                let drunk = solarity_systems::unit_player_inebriation(
                    word(values[3]) as u8,
                    word(values[4]),
                );
                assert_eq!(
                    slot(state.effect(word(values[2]), Some(0), Some(drunk))),
                    integer::<i32>(values[5]),
                    "{line}"
                );
                counts[3] += 1;
            }
            "S" => {
                let gate = word(values[16]);
                let input = UnitWaterSprayInput {
                    foot: Vec3::new(float(values[1]), float(values[2]), float(values[3])),
                    camera: Vec3::new(float(values[4]), float(values[5]), float(values[6])),
                    origin_z: float(values[7]),
                    model_height: float(values[8]),
                    liquid_surface: float(values[9]),
                    speed: f64::from(float(values[10])),
                    walk_speed: float(values[11]),
                    liquid_id: word(values[12]),
                    movement_flags: word(values[13]) | if gate == 3 { 0x4000_0000 } else { 0 },
                    model_flags: word(values[14]),
                    enabled: word(values[15]) != 0,
                    mount_display_id: u32::from(gate == 1),
                    unit_bytes1: if gate == 2 { 0x0002_0000 } else { 0 },
                    player_flags: Some(if gate == 5 { 0x10 } else { 0 }),
                    transport_guid: u64::from(gate == 4),
                };
                let actual = input.resolve();
                assert_eq!(
                    slot(actual.map(|value| value.0)),
                    integer::<i32>(values[17]),
                    "{line}"
                );
                if let Some((_, position)) = actual {
                    assert_eq!(
                        position.to_array().map(f32::to_bits),
                        [float(values[18]), float(values[19]), float(values[20])].map(f32::to_bits),
                        "{line}"
                    );
                }
                counts[2] += 1;
            }
            _ => panic!("unknown fixture kind"),
        }
    }
    assert_eq!(counts, [384, 288, 384, 324]);
}

#[test]
fn unit_breath_stays_cached_until_registration_or_deadline_refresh() {
    let mut state = UnitBreathState::default();
    let mut environment = UnitBreathEnvironment {
        model_height: 2.0,
        unit_scale: 1.0,
        origin_z: 0.0,
        liquid_surface: Some(7.0),
        cold_area: false,
    };
    state.refresh(u32::MAX - 5_000, environment);
    assert_eq!(state.effect(0, Some(0), None), None);
    environment.liquid_surface = Some(7.01);
    assert!(!state.refresh_due(4_998));
    assert_eq!(state.effect(0, Some(0), None), None);
    assert!(state.refresh_due(4_999));
    state.refresh(4_999, environment);
    assert_eq!(
        state.effect(0, Some(0), None),
        Some(UnitWaterEffect::UnderwaterBreath)
    );
    assert!(!state.refresh_due(14_998));
    assert!(state.refresh_due(14_999));
    assert_eq!(UnitWaterEffect::UnderwaterBreath.attachment(), Some(17));
    assert_eq!(UnitWaterEffect::RunSpray.attachment(), None);
}

#[test]
fn unit_effect_scales_match_original_positioned_and_attached_paths() {
    let mut count = 0;
    for line in include_str!("../fixtures/unit-effect-scale-native.txt").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let values: Vec<_> = line.split_ascii_whitespace().map(float).collect();
        assert_eq!(values.len(), 15);
        let world_factor = unit_world_effect_factor(
            Vec3::from_slice(&values[0..3]),
            Vec3::from_slice(&values[3..6]),
            values[6],
        );
        let scale = UnitEffectScale {
            multiplier: values[10],
            minimum: values[11],
            maximum: values[12],
        };
        assert_eq!(
            scale.positioned(world_factor, values[8]).to_bits(),
            values[13].to_bits(),
            "positioned: {line}"
        );
        assert_eq!(
            scale.attached(values[7], f64::from(values[9])).to_bits(),
            values[14].to_bits(),
            "attached: {line}"
        );
        count += 1;
    }
    assert_eq!(count, 384);
}
