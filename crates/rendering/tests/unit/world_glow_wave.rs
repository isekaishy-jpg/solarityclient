use super::*;

#[test]
fn generated_wave_matches_every_native_signed_texel() {
    assert_eq!(
        texture(),
        include_bytes!("../fixtures/world_glow_wave_native.bin")
    );
}

#[test]
fn animated_wave_matches_native_vertices_and_clock_wrap() -> Result<(), Box<dyn std::error::Error>>
{
    let mut cases = 0;
    for line in include_str!("../fixtures/world_glow_wave_coordinates_native.txt").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let words = line
            .split_whitespace()
            .map(|word| u32::from_str_radix(word, 16))
            .collect::<Result<Vec<_>, _>>()?;
        let (width, height, time) = (words[0], words[1], words[2]);
        let rows = transform((width, height), time);
        for vertex in words[3..43].as_chunks::<10>().0 {
            let x = f32::from_bits(vertex[0]) / width as f32;
            let y = 1. - f32::from_bits(vertex[1]) / height as f32;
            for axis in 0..2 {
                let row = rows[axis];
                let uv = (f64::from(row[0]) * (f64::from(x) + 0.5 / 128.)
                    + f64::from(row[1]) * (f64::from(y) + 0.5 / 128.)
                    + f64::from(row[2])) as f32;
                let expected = f32::from_bits(vertex[4 + axis]);
                assert!(
                    (uv - expected).abs() <= 0.000_002,
                    "{width}x{height} time={time} axis={axis} {uv} != {expected}"
                );
            }
        }
        cases += 1;
    }
    assert_eq!(cases, 40);
    Ok(())
}
