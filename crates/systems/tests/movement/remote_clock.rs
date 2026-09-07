//! Sequential captures exercise the retained history as well as each decision.

use super::{RemoteMovementClock, RemoteMovementReceipt};

#[test]
fn delay_history_and_admission_match_original_execution() -> Result<(), Box<dyn std::error::Error>>
{
    let mut clock = RemoteMovementClock::default();
    let mut count = 0;
    for line in include_str!("../fixtures/remote-clock-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let (input, output) = line.split_once(" | ").ok_or("missing capture columns")?;
        let words = |part: &str| {
            part.split_whitespace()
                .map(|word| u32::from_str_radix(word, 16))
                .collect::<Result<Vec<_>, _>>()
        };
        let input = words(input)?;
        let output = words(output)?;
        if input[0] != 0 {
            clock = RemoteMovementClock::default();
        }
        let admitted = clock.admit(RemoteMovementReceipt {
            server_ms: input[1],
            receipt_ms: input[2],
            frame_ms: input[3],
            flags: input[4],
            has_pending_commands: input[5] != 0,
            has_path: input[6] != 0,
        });
        assert_eq!(
            [
                admitted.timeline_ms,
                clock.server_ms,
                clock.delay_ms as u32,
                admitted.adjustment_ms as u32,
                u32::from(admitted.immediate)
            ],
            output.as_slice(),
            "row {count}"
        );
        count += 1;
    }
    assert_eq!(count, 480);
    Ok(())
}
