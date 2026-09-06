//! Original native callback scan with independent primary timer inputs.

use super::M2ModelSequenceTimer;

#[test]
fn sequence_completion_matches_original_callback_scan() -> Result<(), Box<dyn std::error::Error>> {
    let mut cases = 0;
    for row in include_str!("../fixtures/native_sequence_completions.txt").lines() {
        if row.starts_with('#') || row.is_empty() {
            continue;
        }
        let values = row
            .split_whitespace()
            .map(str::parse::<i64>)
            .collect::<Result<Vec<_>, _>>()?;
        let [
            flags,
            duration,
            start,
            end,
            direction,
            previous,
            current,
            paused,
            finished,
            expected,
        ] = values.as_slice()
        else {
            return Err(format!("invalid native completion row: {row}").into());
        };
        let timer = M2ModelSequenceTimer {
            start_ms: *start as u32,
            end_ms: *end as u32,
            duration_ms: *duration as u32,
            cycle_count: 1,
            initial_time_ms: 0,
            speed: *direction as f32,
            inverse_speed: *direction as f32,
            loops: flags & 1 == 0,
            secondary_clamps: flags & 0x80 != 0,
        };
        let completion = (*paused == 0 && *finished == 0)
            .then(|| timer.next_completion_ms(*previous as u32, *current as u32))
            .flatten();
        assert_eq!(completion.map(i64::from).unwrap_or(-1), *expected, "{row}");
        cases += 1;
    }
    assert_eq!(cases, 1_080);
    Ok(())
}
