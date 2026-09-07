//! Compare source-gain and pitch results with the original x87 instruction path.

use super::select_parameters;

#[test]
fn sound_parameters_match_original_executable() -> Result<(), std::num::ParseIntError> {
    let mut cases = 0;
    for line in include_str!("../../../fixtures/sound-parameters-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let values = line
            .split_ascii_whitespace()
            .map(|word| u32::from_str_radix(word, 16))
            .collect::<Result<Vec<_>, _>>()?;
        let mut draws = 0;
        let result = select_parameters(
            f32::from_bits(values[1]),
            f32::from_bits(values[2]),
            values[0],
            &mut || {
                let word = values[3 + draws];
                draws += 1;
                word
            },
        );
        assert_eq!(result.gain.to_bits(), values[5], "gain: {line}");
        assert_eq!(result.frequency_ratio.to_bits(), values[6], "pitch: {line}");
        assert_eq!(draws, values[7] as usize, "draw count: {line}");
        cases += 1;
    }
    assert_eq!(cases, 200);
    Ok(())
}
