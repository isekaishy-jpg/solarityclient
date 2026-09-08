//! Native mesh construction and packed gradient output, without a GPU.

use super::*;
use std::error::Error;

fn unhex(raw: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    raw.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}

#[test]
fn sky_dome_matches_original_geometry_and_gradient() -> Result<(), Box<dyn Error>> {
    let mut palettes = Vec::new();
    let mut meshes = 0;
    let mut gradients = 0;
    for line in include_str!("../fixtures/world_sky_native.txt")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let p = line.split_whitespace().collect::<Vec<_>>();
        match p[0] {
            "mesh" => {
                let dome = WorldSkyDome::with_radius(p[1].parse()?);
                assert_eq!(p[2], "122");
                assert_eq!(p[3], "300");
                let bytes = unhex(p[4])?;
                let native = bytes
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .copied()
                    .map(f32::from_le_bytes);
                for (index, (actual, expected)) in
                    dome.positions.iter().flatten().zip(native).enumerate()
                {
                    assert!(
                        (actual - expected).abs() < 0.000_02,
                        "radius {} component {index}: {actual} != {expected}",
                        p[1]
                    );
                }
                let bytes = unhex(p[5])?;
                let native = bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .copied()
                    .map(u16::from_le_bytes)
                    .collect::<Vec<_>>();
                assert_eq!(dome.indices.as_slice(), native);
                meshes += 1;
            }
            "palette" => {
                let bytes = unhex(p[2])?;
                palettes.push(
                    bytes
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .copied()
                        .map(u32::from_le_bytes)
                        .collect::<Vec<_>>(),
                );
            }
            "gradient" => {
                let palette = &palettes[p[1].parse::<usize>()?];
                let mut dome = WorldSkyDome::new();
                dome.update_packed(
                    palette[..5].try_into()?,
                    palette[5],
                    p[2].parse()?,
                    p[3].parse()?,
                    p[4].parse()?,
                );
                let bytes = unhex(p[5])?;
                for (index, (actual, expected)) in dome
                    .colors
                    .iter()
                    .zip(
                        bytes
                            .as_chunks::<4>()
                            .0
                            .iter()
                            .copied()
                            .map(u32::from_le_bytes),
                    )
                    .enumerate()
                {
                    assert_eq!(
                        *actual,
                        expected,
                        "{} vertex {index}",
                        &line[..line.len().min(50)]
                    );
                }
                gradients += 1;
            }
            "constant" => {}
            "azimuth" => {
                let bytes = unhex(p[1])?;
                let forward = bytes
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .copied()
                    .map(f32::from_le_bytes)
                    .collect::<Vec<_>>();
                let bytes = unhex(p[2])?;
                let expected = f32::from_le_bytes(bytes.as_slice().try_into()?);
                assert!(
                    (view_azimuth(Vec3::from_slice(&forward)) - expected).abs() < 0.000_001,
                    "{line}"
                );
            }
            other => panic!("unknown fixture row {other}"),
        }
    }
    assert_eq!((meshes, gradients), (4, 836));
    Ok(())
}
