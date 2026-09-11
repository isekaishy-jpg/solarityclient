use super::WorldEnvironmentShadowState;
use crate::WorldShadowQuality;
use glam::Vec3;

#[test]
fn environment_updates_match_original_874890() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = WorldEnvironmentShadowState::new(WorldShadowQuality::Disabled);
    let mut cases = 0;
    for line in include_str!("../../../tests/fixtures/world-environment-shadow-native.txt")
        .lines()
        .filter(|line| line.starts_with("environment "))
    {
        let fields: Vec<_> = line.split_whitespace().skip(1).collect();
        let quality = fields[0].parse()?;
        let frame: u32 = fields[1].parse()?;
        if frame == 0 {
            state = WorldEnvironmentShadowState::new(
                WorldShadowQuality::from_cvar(quality).ok_or("invalid fixture quality")?,
            );
        }
        let center = Vec3::new(fields[5].parse()?, fields[6].parse()?, fields[7].parse()?);
        let encoded = fields[8];
        let bytes = (0..encoded.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&encoded[index..index + 2], 16))
            .collect::<Result<Vec<_>, _>>()?;
        let words: Vec<_> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|bytes| u32::from_le_bytes(*bytes))
            .collect();
        assert_eq!(words.len(), 78);
        let updates = state.advance(center)?;
        let equal = |actual: &[f32], offset| {
            let actual: Vec<_> = actual.iter().map(|value| value.to_bits()).collect();
            assert_eq!(
                actual,
                words[offset..offset + actual.len()],
                "quality={quality}, frame={frame}, offset={offset}"
            );
        };
        for (index, update) in updates.iter().enumerate() {
            assert_eq!(update.is_some(), words[3 + index] != 0);
            if let Some(update) = update {
                assert_eq!(update.caster_mask, words[index]);
                equal(&update.center.to_array(), 6 + index * 3);
                equal(&[update.radius], 15 + index);
                equal(&update.crop, 18 + index * 4);
                equal(&update.viewport, 30 + index * 4);
                assert_eq!(
                    update.pixel_viewport(
                        state
                            .quality()
                            .texture_size()
                            .ok_or("enabled fixture quality")?
                    ),
                    words[66 + index * 4..70 + index * 4],
                    "quality={quality}, frame={frame}, map={index}"
                );
            }
            let map = state.maps[index];
            equal(&map.published.to_array(), 42 + index * 8);
            equal(&map.pending.to_array(), 45 + index * 8);
            assert_eq!(map.phase, words[48 + index * 8]);
            assert_eq!(map.buffer as u32, words[49 + index * 8]);
        }
        cases += 1;
    }
    assert_eq!(cases, 576);
    Ok(())
}
