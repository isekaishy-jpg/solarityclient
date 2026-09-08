//! Original cloud mesh, noise tables and incremental pixel output.

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

fn floats(raw: &str) -> Result<Vec<f32>, Box<dyn Error>> {
    Ok(unhex(raw)?
        .as_chunks::<4>()
        .0
        .iter()
        .copied()
        .map(f32::from_le_bytes)
        .collect())
}

#[test]
fn clouds_match_original_incremental_noise_and_pixels() -> Result<(), Box<dyn Error>> {
    let mut cloud = WorldClouds::new(1);
    let mut lighting = WorldCloudLighting::new([0.; 3], [0.; 3], [0.; 3], [0.; 3], 1.);
    let mut frames = 0;
    for line in include_str!("../fixtures/world_cloud_native.txt")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let p = line.split_whitespace().collect::<Vec<_>>();
        match p[0] {
            "tables" => {
                assert_eq!(cloud.noise.values.as_slice(), floats(p[2])?);
                assert_eq!(cloud.noise.smooth.as_slice(), floats(p[3])?);
            }
            "grain" => assert_eq!(cloud.noise.grain.as_slice(), unhex(p[1])?),
            "lighting" => {
                lighting = WorldCloudLighting::new(
                    floats(p[1])?.as_slice().try_into()?,
                    floats(p[2])?.as_slice().try_into()?,
                    floats(p[3])?.as_slice().try_into()?,
                    floats(p[4])?.as_slice().try_into()?,
                    floats(p[5])?[0],
                )
            }
            "mesh" => {
                let mesh = WorldCloudDome::new();
                assert_eq!((p[1], p[2]), ("177", "374"));
                for (a, b) in mesh
                    .positions()
                    .iter()
                    .flatten()
                    .chain(mesh.coordinates().iter().flatten())
                    .zip(floats(p[3])?.into_iter().chain(floats(p[4])?))
                {
                    assert!((a - b).abs() < 0.000_001, "cloud geometry {a} != {b}");
                }
                let colors = unhex(p[5])?;
                assert_eq!(
                    mesh.colors().as_slice(),
                    colors
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .copied()
                        .map(u32::from_le_bytes)
                        .collect::<Vec<_>>()
                );
                let indices = unhex(p[6])?;
                assert_eq!(
                    mesh.indices().as_slice(),
                    indices
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .copied()
                        .map(u16::from_le_bytes)
                        .collect::<Vec<_>>()
                );
            }
            "frame" => {
                if p[4] == "1" {
                    cloud.invalidate();
                }
                cloud.update(p[3].parse()?, p[2].parse()?, lighting);
                let state = unhex(p[7])?;
                let clock = unhex(p[8])?;
                assert_eq!(
                    cloud.current,
                    usize::from(state[3]),
                    "frame {} texture",
                    p[1]
                );
                assert_eq!(
                    cloud.row,
                    u32::from_le_bytes(state[12..16].try_into()?) as usize,
                    "frame {} row",
                    p[1]
                );
                assert_eq!(
                    cloud.phase,
                    u16::from_le_bytes(clock[..2].try_into()?),
                    "frame {} phase",
                    p[1]
                );
                assert_eq!(
                    cloud.elapsed.to_bits(),
                    u32::from_le_bytes(clock[4..8].try_into()?),
                    "frame {} elapsed",
                    p[1]
                );
                let start = p[5].parse::<usize>()? * 128;
                let count = p[6].parse::<usize>()? * 128;
                let expected = unhex(p[10])?;
                for (i, (a, b)) in cloud.alpha[start..start + count]
                    .iter()
                    .zip(expected)
                    .enumerate()
                {
                    assert_eq!(*a, b, "frame {} alpha pixel {}", p[1], start + i);
                }
                for (i, (a, b)) in cloud.previous.iter().zip(floats(p[11])?).enumerate() {
                    assert!(
                        (a - b).abs() < 0.000_001,
                        "frame {} previous {i}: {a} != {b}",
                        p[1]
                    );
                }
                for (i, (a, b)) in cloud.pixels[start * 4..(start + count) * 4]
                    .iter()
                    .zip(unhex(p[9])?)
                    .enumerate()
                {
                    assert_eq!(
                        *a,
                        b,
                        "frame {} color pixel {} component {}",
                        p[1],
                        start + i / 4,
                        i % 4
                    );
                }
                frames += 1;
            }
            "permutation" | "octaves" => {}
            "light_sample" => {
                let rgb = [[102., 136., 170.], [68., 102., 136.], [17., 34., 51.]]
                    .map(|v| glam::Vec3::from_array(v) / 255.);
                let sample = WorldCloudLighting::sample(
                    rgb,
                    p[1].parse()?,
                    glam::Vec3::from_slice(&floats(p[3])?),
                    glam::Vec3::from_slice(&floats(p[4])?),
                    glam::Vec3::from_slice(&floats(p[5])?),
                    p[2].parse()?,
                );
                for (actual, raw) in [
                    sample.ambient,
                    sample.diffuse,
                    sample.emissive,
                    sample.position,
                ]
                .iter()
                .zip(&p[6..10])
                {
                    for (a, b) in actual.iter().zip(floats(raw)?) {
                        assert!(
                            (a - b).abs() < 0.000_02,
                            "cloud light {} {}: {a} != {b}",
                            p[1],
                            p[2]
                        );
                    }
                }
                assert_eq!(sample.strength, floats(p[10])?[0]);
            }
            other => panic!("unknown fixture row {other}"),
        }
    }
    assert_eq!(frames, 39);
    Ok(())
}
