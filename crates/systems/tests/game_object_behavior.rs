//! Original-executable state and progress arithmetic fixtures.

use solarity_systems::{
    GameObjectAnimationState, game_object_reversed_progress, game_object_sequence_offset,
};
use std::error::Error;

#[test]
fn game_object_transition_inputs_match_original_instructions() -> Result<(), Box<dyn Error>> {
    let mut count = 0;
    for line in include_str!("fixtures/game-object-transition-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let operation = words.next().ok_or("missing operation")?;
        let values = words
            .map(str::parse::<u32>)
            .collect::<Result<Vec<_>, _>>()?;
        let progress = |word: u32| (word != u32::from(u16::MAX)).then_some(word as u16);
        match (operation, values.as_slice()) {
            ("initial", &[state, fraction, expected]) => assert_eq!(
                GameObjectAnimationState::initial(state as u8, progress(fraction))
                    .map_or(u32::MAX, |state| state as u32),
                expected,
                "{line}"
            ),
            ("change", &[previous, next, fraction, expected]) => assert_eq!(
                GameObjectAnimationState::changed(previous as u8, next as u8, progress(fraction))
                    .map_or(u32::MAX, |state| state as u32),
                expected,
                "{line}"
            ),
            ("offset", &[duration, fraction, expected]) => assert_eq!(
                game_object_sequence_offset(duration, fraction as u16) as u32,
                expected,
                "{line}"
            ),
            ("reverse", &[now, start, end, called, fraction]) => assert_eq!(
                game_object_reversed_progress(now, start, end),
                (called != 0).then_some(fraction as u16),
                "{line}"
            ),
            _ => return Err(format!("invalid native fixture row: {line}").into()),
        }
        count += 1;
    }
    assert_eq!(count, 232);
    Ok(())
}
