//! Original-instruction model construction and unsigned global-track phases.

use super::M2AnimationClock;

#[test]
fn model_global_ticks_match_original_unsigned_phase_instructions()
-> Result<(), Box<dyn std::error::Error>> {
    let mut count = 0;
    for line in include_str!("../fixtures/model-global-clock.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let fields = line
            .split_whitespace()
            .map(str::parse::<u32>)
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(fields.len(), 4);
        let elapsed = fields[1].wrapping_sub(fields[0]);
        let clock = M2AnimationClock::new_with_global_tick(0, 0.0, elapsed);
        assert_eq!(
            clock.global_sequence_time_ms(fields[2]),
            fields[3] as f32,
            "{line}"
        );
        count += 1;
    }
    assert_eq!(count, 240);
    Ok(())
}
