use super::{WorldEntityLightEnvironment, WorldEntityLightState};
use glam::Vec3;

#[test]
fn entity_transition_and_directional_callback_match_native() {
    let data = include_bytes!("../fixtures/world_entity_light_native.bin");
    let (records, tail) = data.as_chunks::<84>();
    assert!(tail.is_empty());
    assert_eq!(records.len(), 432);
    let environment = WorldEntityLightEnvironment::new(
        Vec3::new(128., 96., 64.) / 255.,
        Vec3::new(0.25, 0.5, 0.75),
        Vec3::new(-0.5, -0.5, -0.70710677),
        Vec3::new(-0.6, -0.6, -0.3),
    );
    for (case, record) in records.iter().enumerate() {
        let words: Vec<_> = record
            .as_chunks::<4>()
            .0
            .iter()
            .map(|word| u32::from_le_bytes(*word))
            .collect();
        let mut state = WorldEntityLightState {
            ambient: words[1].to_le_bytes(),
            target_ambient: words[2].to_le_bytes(),
            diffuse: words[3].to_le_bytes(),
            intensity: f32::from_bits(words[4]),
            target_intensity: f32::from_bits(words[5]),
            interior: words[0] != 0,
            blend_exterior: words[0] == 2,
        };
        for _ in 0..words[7] {
            state.advance(f32::from_bits(words[6]), environment);
        }
        assert_eq!(state.ambient, words[8].to_le_bytes(), "ambient case {case}");
        assert_eq!(state.diffuse, words[9].to_le_bytes(), "diffuse case {case}");
        assert!(
            (state.intensity - f32::from_bits(words[10])).abs() < 0.000001,
            "intensity case {case}"
        );
        assert_eq!(
            state.target_ambient,
            words[11].to_le_bytes(),
            "target case {case}"
        );
        let sample = state.sample(environment);
        for (axis, value) in [sample.ambient(), sample.diffuse(), sample.ray()]
            .into_iter()
            .flat_map(|value| value.to_array())
            .enumerate()
        {
            let expected = f32::from_bits(words[12 + axis]);
            assert!(
                (value - expected).abs() < 0.000001,
                "callback case {case} axis {axis}: {value} vs {expected}"
            );
        }
    }
}
