//! Original manual fog activation, restoration and activation-context latching.

use super::*;

#[test]
fn manual_screen_fog_matches_native_transitions() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = ScreenEffectFog::default();
    let mut cases = 0;
    for line in include_str!("../fixtures/screen_effect_fog_native.txt").lines() {
        let Some(line) = line.strip_prefix("fog ") else {
            continue;
        };
        let row = line.split_whitespace().collect::<Vec<_>>();
        if row[5] == "0" {
            state = ScreenEffectFog::default();
        }
        let context = WorldFogContext::new(if row[1] == "1" { 530 } else { 0 }, row[0].parse()?)
            .ok_or("context")?;
        let id = row[6].parse::<i32>()?;
        if id >= 0 {
            let kind = match id {
                0 => None,
                1..=4 => Some(id as u32 - 1),
                _ => Some(99),
            };
            state.select(kind, Some(context), row[4] == "1");
        }
        let manual = state.resolve(context);
        assert_eq!(manual.is_some(), row[12] == "00000001", "{line}");
        assert_eq!(manual.is_none(), row[11] == "00000001", "{line}");
        if let Some(manual) = manual {
            let fog = context.resolve_manual_fog(manual, row[2] == "1");
            let range = fog.range();
            for (actual, value) in [range.0, range.1, fog.exponent()]
                .into_iter()
                .zip(&row[8..11])
            {
                assert_eq!(actual.to_bits(), u32::from_str_radix(value, 16)?, "{line}");
            }
            let color = fog
                .color()
                .to_array()
                .map(|channel| (channel * 255.).round() as u32);
            let packed = 0xff000000 | color[0] << 16 | color[1] << 8 | color[2];
            assert_eq!(packed, u32::from_str_radix(row[7], 16)?, "{line}");
        }
        cases += 1;
    }
    assert_eq!(cases, 1616);
    Ok(())
}

#[test]
fn manual_screen_fog_waits_for_the_first_available_camera_context()
-> Result<(), Box<dyn std::error::Error>> {
    let mut state = ScreenEffectFog::default();
    state.select(Some(2), None, false);
    state.select(Some(99), None, true);
    let context = WorldFogContext::new(0, 777.).ok_or("context")?;
    let manual = state.resolve(context).ok_or("manual")?;
    let fog = context.resolve_manual_fog(manual, false);
    assert_eq!(fog.range(), (105., 150.));
    assert_eq!(fog.color(), glam::Vec3::new(76., 76., 99.) / 255.);
    state.select(None, Some(context), true);
    assert!(state.resolve(context).is_none());
    Ok(())
}
