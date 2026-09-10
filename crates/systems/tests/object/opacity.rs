use super::{EntityOpacity, EntityRetirement};

fn records(source: &str) -> impl Iterator<Item = Vec<u32>> + '_ {
    source
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| {
            line.split_whitespace()
                .map(|word| {
                    u32::from_str_radix(word, 16).unwrap_or_else(|error| {
                        panic!("invalid native fixture word {word}: {error}")
                    })
                })
                .collect()
        })
}

#[test]
fn setters_and_updates_match_both_native_entity_owners() {
    let mut count = 0;
    for row in records(include_str!("../fixtures/entity_opacity_native.txt")) {
        assert_eq!(row.len(), 17);
        let mut opacity = EntityOpacity {
            started_at: row[6],
            duration_ms: row[7],
            current: row[8] as u8,
            from: row[9] as u8,
            target: row[10] as u8,
            multiplier: row[5] as u8,
        };
        match row[0] {
            0 => opacity.select(f32::from_bits(row[2]), row[3], row[1]),
            1 if row[4] != 0 => opacity.advance(row[1]),
            1 => {}
            _ => unreachable!(),
        }
        assert_eq!(
            [
                opacity.started_at,
                opacity.duration_ms,
                u32::from(opacity.current),
                u32::from(opacity.from),
                u32::from(opacity.target)
            ],
            row[11..16],
            "record {count}: {row:x?}",
        );
        if row[0] == 1 && row[4] != 0 {
            assert_eq!(opacity.opacity().to_bits(), row[16], "record {count}");
        }
        count += 1;
    }
    assert_eq!(count, 1189);
}

#[test]
fn detached_opacity_and_retirement_match_native_scene_update() {
    let mut count = 0;
    for row in records(include_str!("../fixtures/entity_retirement_native.txt")) {
        let actual =
            EntityRetirement::new(row[1], f32::from_bits(row[2])).sample(row[0], row[3] != 0);
        if row[4] != 0 {
            assert_eq!(actual, None, "record {count}");
        } else {
            assert_eq!(actual.map(f32::to_bits), Some(row[5]), "record {count}");
        }
        count += 1;
    }
    assert_eq!(count, 352);
}

#[test]
fn unit_entry_eligibility_matches_native_flags_transports_and_vehicle_seats() {
    let mut count = 0;
    for row in records(include_str!("../fixtures/entity_entry_policy_native.txt")) {
        let duration = EntityOpacity::unit_entry_duration(
            row[0],
            row[1],
            row[2],
            u64::from(row[3]) | (u64::from(row[4]) << 32),
            (row[5] != 0).then_some(row[6] != 0),
            (row[7] != 0).then_some(row[8]),
        );
        assert_eq!(duration, row[9] * 1000, "record {count}: {row:x?}");
        count += 1;
    }
    assert_eq!(count, 560);
}
