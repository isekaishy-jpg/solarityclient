//! Exact selector and completion records from the pinned client instructions.

use solarity_systems::{VehiclePassengerAnimationInput, VehiclePassengerPhase as Phase};

#[test]
fn passenger_animation_selection_and_completion_match_native()
-> Result<(), Box<dyn std::error::Error>> {
    let mut count = 0;
    for line in include_str!("fixtures/vehicle-animation-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let values = line
            .split_whitespace()
            .map(|word| u32::from_str_radix(word, 16))
            .collect::<Result<Vec<_>, _>>()?;
        let phase = match values[0] {
            0 => Phase::Detached,
            1 => Phase::EnterDelay,
            2 => Phase::Entering,
            3 => Phase::Seated,
            4 => Phase::ExitDelay,
            5 => Phase::Exiting,
            _ => return Err("invalid native phase".into()),
        };
        let pair = |at| [values[at] as i32, values[at + 1] as i32];
        let mut input = VehiclePassengerAnimationInput {
            phase,
            flags: values[1],
            completed: values[2],
            special_exit: values[3] != 0,
            enter: pair(5),
            seated: pair(7),
            secondary: pair(9),
            exit: pair(11),
        };
        let word = |selected: Option<i32>| selected.unwrap_or(506) as u32;
        let mut actual = vec![
            word(input.primary()),
            word(input.secondary()),
            word(input.before_movement(values[4] != 0)),
            word(input.after_movement()),
        ];
        input.completed = input.complete(values[13] as i32);
        actual.extend([
            input.completed,
            word(input.primary()),
            word(input.secondary()),
        ]);
        assert_eq!(actual, values[14..], "native record {count}: {line}");
        count += 1;
    }
    assert_eq!(count, 5120);
    Ok(())
}
