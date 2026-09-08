//! Original speed/pose/deadline instructions, independent of Rust timer math.

use super::{M2ModelAnimationMode, M2ModelSequenceTimer, M2SequenceStartPhase};

#[test]
fn sequence_seek_matches_original_client() -> Result<(), Box<dyn std::error::Error>> {
    let mut cases = 0;
    for line in include_str!("../fixtures/native_model_sequence_seek.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let values = line
            .split_whitespace()
            .map(str::parse::<i64>)
            .collect::<Result<Vec<_>, _>>()?;
        let mode = match values[1] {
            0 => M2ModelAnimationMode::Forward,
            1 => M2ModelAnimationMode::Reverse,
            2 => M2ModelAnimationMode::HoldStart,
            3 => M2ModelAnimationMode::HoldEnd,
            _ => return Err("invalid mode".into()),
        };
        let mut timer = M2ModelSequenceTimer::with_timing(
            values[0] as u32,
            3,
            0x20,
            mode,
            f32::from_bits(values[2] as u32),
            values[4] as u32,
            values[3] as i32,
            if values[5] == 0 {
                M2SequenceStartPhase::BeforeSceneUpdate
            } else {
                M2SequenceStartPhase::DuringSceneUpdate
            },
        );
        timer.seek(values[7] as i32, values[6] as u32);
        let actual = [
            timer.start_ms,
            timer.end_ms,
            timer.speed.to_bits(),
            timer.inverse_speed.to_bits(),
            timer.initial_time_ms,
            timer.cycle_count,
        ];
        let expected = values[8..]
            .iter()
            .map(|word| *word as u32)
            .collect::<Vec<_>>();
        assert_eq!(actual.as_slice(), expected, "{line}");
        cases += 1;
    }
    assert_eq!(cases, 2880);
    Ok(())
}

#[test]
fn sequence_speed_matches_original_client() -> Result<(), Box<dyn std::error::Error>> {
    let mut cases = 0;
    for line in include_str!("../fixtures/native_model_sequence_speed.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let row: Vec<_> = line.split_whitespace().collect();
        let values = row[1..]
            .iter()
            .map(|value| value.parse::<i64>())
            .collect::<Result<Vec<_>, _>>()?;
        let mode = match values[1] {
            0 => M2ModelAnimationMode::Forward,
            1 => M2ModelAnimationMode::Reverse,
            2 => M2ModelAnimationMode::HoldStart,
            3 => M2ModelAnimationMode::HoldEnd,
            _ => return Err("invalid native mode".into()),
        };
        let flags = if matches!(row[0], "pose" | "boundary") {
            values[6] as u32
        } else {
            0x20
        };
        let mut timer = M2ModelSequenceTimer::with_timing(
            values[0] as u32,
            3,
            flags,
            mode,
            f32::from_bits(values[2] as u32),
            values[4] as u32,
            values[3] as i32,
            if values[5] == 0 {
                M2SequenceStartPhase::BeforeSceneUpdate
            } else {
                M2SequenceStartPhase::DuringSceneUpdate
            },
        );
        match row[0] {
            "setup" | "update" | "variation" => {
                let offset = if row[0] == "update" {
                    timer.set_speed(f32::from_bits(values[7] as u32), values[6] as u32);
                    8
                } else if row[0] == "variation" {
                    timer = M2ModelSequenceTimer::with_timing(
                        values[0] as u32,
                        3,
                        0x20,
                        mode,
                        timer.speed,
                        values[6] as u32,
                        timer.variation_time_offset(values[7] as u32),
                        M2SequenceStartPhase::DuringSceneUpdate,
                    );
                    8
                } else {
                    6
                };
                let actual = [
                    timer.start_ms,
                    timer.end_ms,
                    timer.speed.to_bits(),
                    timer.inverse_speed.to_bits(),
                    timer.initial_time_ms,
                    timer.cycle_count,
                ];
                let expected: Vec<_> = values[offset..].iter().map(|value| *value as u32).collect();
                assert_eq!(actual.as_slice(), expected, "{line}");
            }
            "pose" => assert_eq!(
                timer.animation_time_ms(values[7] as u32),
                values[8] as u32,
                "{line}"
            ),
            "event" => assert_eq!(
                timer.event_tick_ms(values[6] as u32, values[7] as u32),
                values[8] as u32,
                "{line}"
            ),
            "boundary" => assert_eq!(
                timer
                    .next_completion_ms(values[7] as u32, values[8] as u32)
                    .map(i64::from)
                    .unwrap_or(-1),
                values[9],
                "{line}"
            ),
            _ => return Err(format!("invalid native speed row: {line}").into()),
        }
        cases += 1;
    }
    assert_eq!(cases, 11760);
    Ok(())
}
