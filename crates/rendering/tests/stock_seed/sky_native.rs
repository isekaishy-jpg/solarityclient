//! Native mesh construction and packed gradient output, without a GPU.

use super::*;
use std::error::Error;

/// Native draw admission includes strict viewport intersection and D3D9's
/// asymmetric lower/upper pixel rounding, without changing sky projection.
#[test]
fn sky_portal_windows_match_native_draw_gate_and_backbuffer_scissor() -> Result<(), Box<dyn Error>>
{
    use crate::{WorldScreenWindow, WorldSkyWindow};
    let mut count = 0;
    for line in include_str!("../fixtures/world_sky_window_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        let floats = fields[4..12]
            .iter()
            .map(|field| Ok(f32::from_bits(u32::from_str_radix(field, 16)?)))
            .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
        let view = floats[..4]
            .iter()
            .map(|value| value * 2. - 1.)
            .collect::<Vec<_>>();
        let viewport = WorldScreenWindow::new(view[1], view[0], view[3], view[2]);
        let input = [floats[4], floats[5], floats[6], floats[7]];
        let window = if fields[1] == "1" {
            WorldSkyWindow::new(input)
                .ok()
                .and_then(|window| window.clipped(viewport))
        } else {
            None
        };
        if fields[14] == "-" {
            assert!(window.is_none(), "{line}");
            assert_eq!(&fields[12..14], ["0", "0"]);
        } else {
            let window = window.ok_or("native sky window was rejected")?;
            assert_eq!(&fields[12..14], ["6", "11345"]);
            for (actual, expected) in window.bounds().into_iter().zip(&fields[14..18]) {
                assert_eq!(
                    actual.to_bits(),
                    u32::from_str_radix(expected, 16)?,
                    "{line}"
                );
            }
            let pixels = window.pixel_bounds([fields[2].parse()?, fields[3].parse()?]);
            for (actual, expected) in pixels.into_iter().zip(&fields[18..22]) {
                assert_eq!(actual, expected.parse::<u32>()?, "{line}");
            }
        }
        count += 1;
    }
    assert_eq!(count, 160);
    Ok(())
}

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
