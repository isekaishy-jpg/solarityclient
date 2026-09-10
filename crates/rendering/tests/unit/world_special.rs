use super::*;

#[test]
fn special_cursor_matches_native_seed_mesh_through_wrap() {
    let fixture = include_bytes!("../fixtures/special_seed_native.bin");
    assert_eq!(&fixture[..4], b"SPSD");
    let word = |at| {
        u32::from_le_bytes([
            fixture[at],
            fixture[at + 1],
            fixture[at + 2],
            fixture[at + 3],
        ])
    };
    let mut state = WorldSpecialState::default();
    for index in 0..word(4) as usize {
        if index == 135 {
            state.select([0xff000000, 1, 100, 0]);
        }
        let frame = state.advance(0.);
        let offset = 8 + index * 132;
        assert_eq!(frame.seed_row, word(offset));
        assert_eq!(frame.clear_history, matches!(index, 0 | 135));
        for vertex in 0..4 {
            assert_eq!(
                f32::from_bits(word(offset + 4 + vertex * 20 + 16)),
                (frame.seed_row as f32 + 0.5) / 256.
            );
        }
        assert_eq!(f32::from_bits(word(offset + 8)), 3.);
        assert_eq!(f32::from_bits(word(offset + 28)), 4.);
    }
    assert_eq!(fixture.len(), 8 + word(4) as usize * 132);
}

#[test]
fn special_noise_matches_every_native_texture_byte() {
    assert_eq!(
        super::super::special_noise::texture(),
        include_bytes!("../fixtures/special_noise_native.rgba")
    );
    assert_eq!(
        include_bytes!(concat!(env!("OUT_DIR"), "/special-noise.rgba")),
        include_bytes!("../fixtures/special_noise_native.rgba")
    );
}

#[test]
fn special_selection_and_ramp_match_native_float_stores() -> Result<(), Box<dyn std::error::Error>>
{
    let mut state = WorldSpecialState::default();
    for line in include_str!("../fixtures/special_state_native.txt").lines() {
        if line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let kind = fields.next().ok_or("native row kind")?;
        let words = fields
            .map(|word| u32::from_str_radix(word, 16))
            .collect::<Result<Vec<_>, _>>()?;
        if kind == "select" {
            state.select([words[0], words[1], words[2], 0]);
            assert_eq!(
                [
                    state.color,
                    state.decay.to_bits(),
                    state.strength.to_bits(),
                    state.countdown.to_bits()
                ],
                words[3..7]
            );
        } else {
            let frame = state.advance(f32::from_bits(words[0]));
            assert_eq!(state.countdown.to_bits(), words[1]);
            assert!(
                words[2..6]
                    .iter()
                    .all(|word| *word == frame.desaturation.to_bits())
            );
            assert!(
                words[6..10]
                    .iter()
                    .all(|word| *word == frame.whitening.to_bits())
            );
        }
    }
    Ok(())
}
