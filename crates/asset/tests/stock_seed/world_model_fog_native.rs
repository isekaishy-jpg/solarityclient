//! Native four-volume MFOG selection, including duplicate and equal-distance IDs.

use super::*;
use std::error::Error;

fn unhex(text: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    text.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}

#[test]
fn world_model_fog_palette_matches_original_heap_and_two_bank_blending()
-> Result<(), Box<dyn Error>> {
    let mut volumes = Vec::new();
    let mut count = 0;
    for line in include_str!("../fixtures/world_model_fog_native.txt").lines() {
        let row = line.split_ascii_whitespace().collect::<Vec<_>>();
        match row.first().copied() {
            Some("volumes") => {
                volumes.clear();
                for record in unhex(row[2])?.as_chunks::<48>().0 {
                    let words = record
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .copied()
                        .map(u32::from_le_bytes)
                        .collect::<Vec<_>>();
                    let f = |i| f32::from_bits(words[i]);
                    volumes.push(WorldModelFog {
                        flags: words[0],
                        position: Vec3::new(f(1), f(2), f(3)),
                        inner_radius: f(4),
                        outer_radius: f(5),
                        banks: [
                            WorldModelFogBank::new(f(6), f(7), words[8]),
                            WorldModelFogBank::new(f(9), f(10), words[11]),
                        ],
                    });
                }
            }
            Some("palette") => {
                let indices: [u8; 4] = unhex(row[2])?.try_into().map_err(|_| "invalid indices")?;
                let f = |i: usize| -> Result<f32, Box<dyn Error>> {
                    Ok(f32::from_bits(u32::from_str_radix(row[i], 16)?))
                };
                let sample =
                    sample_world_model_fog(&volumes, indices, Vec3::new(f(3)?, f(4)?, f(5)?))
                        .ok_or("missing palette")?;
                let native = unhex(row[6])?
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .copied()
                    .map(u32::from_le_bytes)
                    .collect::<Vec<_>>();
                assert_eq!(sample.flags(), native[0], "{line}");
                for (bank, expected) in sample.banks().iter().zip(native[1..].as_chunks::<3>().0) {
                    assert_eq!(
                        [bank.end.to_bits(), bank.start_ratio.to_bits(), bank.color],
                        *expected,
                        "{line}"
                    );
                }
                count += 1;
            }
            _ => {}
        }
    }
    assert_eq!(count, 288);
    Ok(())
}
