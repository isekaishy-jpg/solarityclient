//! Native primary timer inputs captured while executing the full model callback.

use super::M2ModelSequenceTimer;
use std::error::Error;

#[test]
fn full_ground_alignment_weights_match_original_primary_timer() -> Result<(), Box<dyn Error>> {
    for (index, line) in include_str!("../fixtures/unit-ground-pose-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .enumerate()
    {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        let flags: u32 = fields[9].parse()?;
        let speed = f32::from_bits(u32::from_str_radix(fields[10], 16)?);
        let timer = M2ModelSequenceTimer {
            start_ms: fields[12].parse()?,
            end_ms: fields[13].parse()?,
            duration_ms: 1000,
            cycle_count: 1,
            initial_time_ms: fields[11].parse()?,
            speed,
            inverse_speed: 0.0,
            loops: false,
            secondary_clamps: false,
        };
        let actual = timer.ground_alignment_weight(flags, fields[14].parse()?);
        let expected = u32::from_str_radix(fields[15], 16)?;
        assert_eq!(actual.to_bits(), expected, "primary timer case {index}");
    }
    Ok(())
}
