use super::*;

#[test]
fn native_wound_routes_preserve_the_primary_animation() -> Result<(), std::num::ParseIntError> {
    let fixture = include_str!("../fixtures/unit_wound_native.routes.txt");
    let mut count = 0;
    for line in fixture
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let fields: Vec<i64> = line
            .split_whitespace()
            .map(str::parse)
            .collect::<Result<_, _>>()?;
        assert_eq!(fields.len(), 22);
        let current = if fields[6] == -1 || fields[7] == -1 {
            fields[5]
        } else {
            fields[6]
        };
        let input = UnitWoundAnimationInput {
            critical: fields[0] != 0,
            attack_target_guid: fields[1] as u64,
            movement_flags: (fields[2] | fields[14]) as u32,
            stand: fields[3] as u8,
            mounted: fields[4] != 0,
            primary_animation: fields[5] as u16,
            primary_behavior: fields[5] as u16,
            current_animation: current as u16,
            current_behavior: current as u16,
            upper_body_key_bone: u16::try_from(fields[7]).ok(),
            dead: fields[8] != 0,
            model_ready: fields[9] != 0 && fields[11] != 0,
            template_flags: u32::try_from(fields[12]).ok(),
            effect_blocks_wound: fields[13] & 4 != 0,
            secondary_movement_flags: fields[15] as u16,
            movement_handler_flags: fields[16] as u32,
            vehicle_blocks_wound: fields[17] != 0,
        };
        let selected = resolve_unit_wound_animation(input).filter(|_| fields[10] != 0);
        assert_eq!(
            selected.map_or(-1, |value| i64::from(value.animation)),
            fields[18],
            "{line}"
        );
        assert_eq!(
            selected.map_or(-2, |value| value.key_bone.map_or(-1, i64::from)),
            fields[19],
            "{line}"
        );
        if selected.is_some() {
            assert_eq!(&fields[20..], &[1, 0]);
        }
        count += 1;
    }
    assert_eq!(count, 68);
    Ok(())
}
