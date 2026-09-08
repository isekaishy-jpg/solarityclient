//! Original machine-code captures, including horizon and fade thresholds.
use super::*;
use std::error::Error;

fn words(raw: &str) -> Result<Vec<u32>, Box<dyn Error>> {
    let bytes = raw
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| Ok(u8::from_str_radix(std::str::from_utf8(p)?, 16)?))
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    Ok(bytes
        .as_chunks::<4>()
        .0
        .iter()
        .copied()
        .map(u32::from_le_bytes)
        .collect())
}

#[test]
fn celestial_colors_keep_native_weather_and_second_moon_state() -> Result<(), Box<dyn Error>> {
    let mut lighting = crate::WorldCelestialLighting::default();
    for line in include_str!("../fixtures/world_celestial_color_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let p = line.split_whitespace().collect::<Vec<_>>();
        if p[0] == "initial" {
            assert_eq!(lighting.colors().as_slice(), words(p[1])?);
        } else {
            let input = words(p[1])?;
            let color = Vec3::new(
                ((input[0] >> 16) & 255) as f32,
                ((input[0] >> 8) & 255) as f32,
                (input[0] & 255) as f32,
            ) / 255.;
            lighting.update(color, f32::from_bits(input[1]));
            assert_eq!(lighting.colors().as_slice(), words(p[2])?, "{line}");
        }
    }
    Ok(())
}

#[test]
fn celestial_mesh_and_basis_match_original_machine_code() -> Result<(), Box<dyn Error>> {
    let mut meshes = 0;
    let mut bases = 0;
    for line in include_str!("../fixtures/world_celestial_mesh_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let p = line.split_whitespace().collect::<Vec<_>>();
        let input = words(p[1])?;
        let expected = words(p[2])?;
        let f = |i| f32::from_bits(input[i]);
        match p[0] {
            "mesh" => {
                let body = WorldCelestialBody {
                    position: Vec3::new(0., 0., f(0)),
                    size: f(2),
                };
                let mesh = WorldCelestialMesh::new(body, Vec3::new(0., 0., f(1)), input[3]);
                let actual = [mesh.vertex_count as u32, mesh.index_count as u32]
                    .into_iter()
                    .chain(mesh.positions.into_iter().flatten().map(f32::to_bits))
                    .chain(mesh.uv.into_iter().flatten().map(f32::to_bits))
                    .chain(mesh.colors)
                    .chain(
                        mesh.indices
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .map(|p| u32::from(p[0]) | u32::from(p[1]) << 16),
                    )
                    .collect::<Vec<_>>();
                for (index, (a, b)) in actual.iter().zip(&expected).enumerate() {
                    // Native x87 intermediates have 64 significand bits; Rust f64
                    // has 53. Final geometric ties can differ by one float ULP.
                    if (2..32).contains(&index) {
                        assert!(
                            a.abs_diff(*b) <= 1,
                            "mesh {meshes}, word {index}, input {input:x?}: {} != {}",
                            f32::from_bits(*a),
                            f32::from_bits(*b)
                        );
                    } else {
                        assert_eq!(a, b, "mesh {meshes}, word {index}, input {input:x?}");
                    }
                }
                meshes += 1;
            }
            "basis" => {
                let actual = basis(Vec3::new(f(0), f(1), f(2)))
                    .to_cols_array()
                    .map(f32::to_bits);
                assert_eq!(
                    actual.as_slice(),
                    expected,
                    "basis {bases}, input {input:x?}"
                );
                bases += 1;
            }
            _ => return Err("unknown native fixture record".into()),
        }
    }
    assert!(meshes >= 1900 && bases >= 260);
    Ok(())
}
