//! Native principal-height collision recovery histories.

use super::CameraHeightTransition;

#[test]
fn camera_height_recovery_matches_original_histories() -> Result<(), Box<dyn std::error::Error>> {
    for row in include_str!("../fixtures/camera-height-recovery-native.txt")
        .lines()
        .filter(|row| !row.starts_with('#') && !row.is_empty())
    {
        let groups = row
            .split('|')
            .map(|part| {
                part.split_whitespace()
                    .map(|value| u32::from_str_radix(value, 16))
                    .collect::<Result<Vec<_>, _>>()
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut state = CameraHeightTransition::new(f32::from_bits(groups[0][0]));
        for (action, expected) in groups[1]
            .as_chunks::<3>()
            .0
            .iter()
            .zip(groups[2].as_chunks::<5>().0)
        {
            if action[0] == 1 {
                state.obstructed(f32::from_bits(action[2]), action[1]);
            } else {
                state.advance_collision(action[1]);
            }
            assert_eq!(state.current.to_bits(), expected[0], "{row}");
            assert_eq!(state.target.to_bits(), expected[1], "{row}");
            assert_eq!(
                u32::from(state.collision_started.is_some()),
                expected[2],
                "{row}"
            );
            assert_eq!(state.collision_started.unwrap_or(0), expected[3], "{row}");
            if state.collision_started.is_some() {
                assert_eq!(state.start.to_bits(), expected[4], "{row}");
            }
        }
    }
    Ok(())
}
