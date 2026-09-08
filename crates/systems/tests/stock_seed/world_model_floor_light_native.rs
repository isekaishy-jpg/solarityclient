use super::{interpolate, split_color};
use glam::Vec3;

#[test]
fn floor_daylight_targets_and_doodad_split_match_original_captures() {
    use crate::{WorldEntityLightEnvironment, WorldEntityLightState};
    let environment = WorldEntityLightEnvironment::new(
        Vec3::new(128., 96., 64.) / 255.,
        Vec3::new(32., 64., 128.) / 255.,
        -Vec3::Z,
        -Vec3::Z,
    );
    let data = include_bytes!("../fixtures/world_entity_floor_target_native.bin");
    let (records, tail) = data.as_chunks::<28>();
    assert!(tail.is_empty());
    assert_eq!(records.len(), 384);
    for (case, record) in records.iter().enumerate() {
        let words: Vec<_> = record
            .as_chunks::<4>()
            .0
            .iter()
            .map(|bytes| u32::from_le_bytes(*bytes))
            .collect();
        let color = words[0].to_le_bytes();
        let mut state = WorldEntityLightState::new(environment);
        state.set_floor(
            true,
            Some(super::WorldModelFloorLight {
                color,
                exterior: words[1] != 0,
            }),
            environment,
        );
        state.advance(1., environment);
        let actual = state.sample(environment);
        for (actual, expected) in [(actual.diffuse(), words[2]), (actual.ambient(), words[3])] {
            let bytes = expected.to_le_bytes();
            let expected = Vec3::new(bytes[2].into(), bytes[1].into(), bytes[0].into()) / 255.;
            assert!(
                (actual - expected).abs().max_element() < 0.000001,
                "target case {case}: {actual:?} != {expected:?}"
            );
        }
        assert_eq!(words[4] & 0x1000 != 0, words[1] != 0);
        assert_eq!(
            super::world_model_doodad_light_colors(color),
            [words[5].to_le_bytes(), words[6].to_le_bytes()],
            "MODD split case {case}"
        );
    }
}

#[test]
fn floor_interpolation_and_entity_split_match_original_captures()
-> Result<(), Box<dyn std::error::Error>> {
    let data = include_bytes!("../fixtures/world_model_floor_light_native.bin");
    let (records, tail) = data.as_chunks::<92>();
    assert!(tail.is_empty());
    assert_eq!(records.len(), 768);
    for (case, record) in records.iter().enumerate() {
        let words: Vec<_> = record
            .as_chunks::<4>()
            .0
            .iter()
            .map(|bytes| u32::from_le_bytes(*bytes))
            .collect();
        let vertex = |offset| {
            Vec3::from_array(std::array::from_fn(|axis| {
                f32::from_bits(words[offset + axis])
            }))
        };
        let expected = words[19].to_le_bytes();
        let actual = if words[17] == 65535 {
            words[15].to_le_bytes()
        } else {
            interpolate(
                [vertex(0), vertex(3), vertex(6)],
                vertex(9),
                [
                    words[12].to_le_bytes(),
                    words[13].to_le_bytes(),
                    words[14].to_le_bytes(),
                ],
                words[15].to_le_bytes(),
                words[16] as u16,
            )
            .ok_or("nondegenerate native triangle")?
        };
        assert_eq!(actual, expected, "interpolation case {case}");
        assert_eq!(
            split_color(actual, 168),
            [words[20].to_le_bytes(), words[21].to_le_bytes()],
            "color split case {case}"
        );
        assert_eq!(
            words[22],
            if words[17] == 65535 { 0 } else { words[18] & 1 }
        );
    }
    Ok(())
}
