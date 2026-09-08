//! Native x87 day-band arithmetic, including ties and cyclic key boundaries.

use super::*;

#[test]
fn light_bands_match_original_x87_captures() -> Result<(), Box<dyn std::error::Error>> {
    let mut count = 0;
    for line in include_str!("../fixtures/world_light_sampling_native.txt")
        .lines()
        .filter(|line| line.starts_with("band "))
    {
        let row = line.split_ascii_whitespace().collect::<Vec<_>>();
        let decode = |hex: &str| -> Result<Vec<u32>, Box<dyn std::error::Error>> {
            let bytes = hex
                .as_bytes()
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
                .collect::<Result<Vec<u8>, Box<dyn std::error::Error>>>()?;
            Ok(bytes
                .as_chunks::<4>()
                .0
                .iter()
                .copied()
                .map(u32::from_le_bytes)
                .collect())
        };
        let raw = decode(row[4])?;
        let band = LightBand {
            entries: raw[1] as usize,
            times: raw[2..18].try_into()?,
            values: raw[18..34].try_into()?,
        };
        let expected = decode(row[5])?[0];
        let time = row[3].parse()?;
        let actual = if row[1] == "color" {
            sample_color_band(&band, time)
        } else {
            let floats = LightBand {
                entries: band.entries,
                times: band.times,
                values: band.values.map(f32::from_bits),
            };
            sample_float_band(
                &floats,
                time,
                if row[2] == "0" {
                    CLIENT_COORDINATE_SCALE
                } else {
                    1.0
                },
            )
            .to_bits()
        };
        assert_eq!(actual, expected, "{line}");
        count += 1;
    }
    assert_eq!(count, 272);
    Ok(())
}
