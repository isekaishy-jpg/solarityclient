use super::*;

#[test]
fn weather_transitions_match_original_interrupted_and_instant_captures()
-> Result<(), Box<dyn std::error::Error>> {
    let mut state = WeatherTransition::default();
    let mut counts = [0; 4];
    for line in include_str!("../fixtures/world_weather_transition_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let row = line.split_ascii_whitespace().collect::<Vec<_>>();
        let hex_float = |word| -> Result<f32, Box<dyn std::error::Error>> {
            Ok(f32::from_bits(u32::from_str_radix(word, 16)?))
        };
        let raw = match row[0] {
            "reset" => {
                let grade = hex_float(row[2])?;
                let weight = hex_float(row[3])?;
                state = WeatherTransition {
                    kind: row[1].parse()?,
                    target_grade: grade,
                    previous_grade: grade,
                    grade,
                    target_light: grade.min(0.25),
                    previous_light: grade.min(0.25),
                    light: grade.min(0.25),
                    started: 0,
                    weight,
                    previous_weight: weight,
                    target_weight: weight,
                    force_update: false,
                };
                counts[0] += 1;
                continue;
            }
            "set" => {
                state.receive(
                    row[2].parse()?,
                    hex_float(row[3])?,
                    row[4] == "1",
                    hex_float(row[5])?,
                    row[1].parse()?,
                );
                counts[1] += 1;
                row[6]
            }
            "tick" | "frame" => {
                if row[0] == "tick" {
                    state.interpolate(row[1].parse()?);
                    counts[2] += 1;
                } else {
                    state.sample(row[1].parse()?);
                    counts[3] += 1;
                }
                let expected = u32::from_str_radix(row[3], 16)?.swap_bytes();
                assert_eq!(
                    (state.light * state.weight).to_bits(),
                    expected,
                    "{line}: published light"
                );
                row[2]
            }
            _ => continue,
        };
        let bytes = raw
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
            .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
        let words = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .copied()
            .map(u32::from_le_bytes)
            .collect::<Vec<_>>();
        for (actual, index) in [
            (state.target_grade, 0),
            (state.previous_grade, 1),
            (state.grade, 2),
            (state.target_light, 3),
            (state.previous_light, 4),
            (state.light, 5),
            (state.weight, 10),
            (state.previous_weight, 11),
            (state.target_weight, 12),
        ] {
            assert_eq!(actual.to_bits(), words[index], "{line}: field {index}");
        }
        assert_eq!(state.started, words[7], "{line}: lighting anchor");
    }
    assert_eq!(counts, [56, 248, 672, 144]);
    Ok(())
}
