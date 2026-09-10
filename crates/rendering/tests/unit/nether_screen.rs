use super::WorldNetherState;

#[test]
fn invisibility_state_matches_native_banks_colors_orientation_and_reactivation()
-> Result<(), Box<dyn std::error::Error>> {
    let mut state = WorldNetherState::default();
    for (index, line) in include_str!("../fixtures/nether_screen_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .enumerate()
    {
        let words = line
            .split_whitespace()
            .map(|word| u32::from_str_radix(word, 16))
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(words.len(), 153);
        if words[1] != 0 {
            state.reset_fade();
        }
        let frame = state.advance(
            f32::from_bits(words[0]),
            [words[2], words[3], words[4]].map(f32::from_bits),
        );
        assert_eq!(state.time.to_bits(), words[5], "time frame {index}");
        assert_eq!(state.phase.to_bits(), words[6], "phase frame {index}");
        assert!(
            (frame.angle - f32::from_bits(words[7])).abs() < 0.000002,
            "angle frame {index}"
        );
        assert_eq!(frame.fade.to_bits(), words[8], "fade frame {index}");
        assert_eq!(
            frame.colors.map(u32::from).as_slice(),
            &words[9..45],
            "colors frame {index}"
        );
        assert!(
            state
                .banks
                .iter()
                .flatten()
                .zip(&words[45..])
                .all(|(actual, expected)| actual.to_bits() == *expected),
            "banks frame {index}"
        );
    }
    Ok(())
}
