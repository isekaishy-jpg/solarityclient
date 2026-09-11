//! Captured native pairing decisions and shared spheres, including flag short circuits.

use super::{BatchLayer, pairing, sphere};
use glam::Vec3;

#[test]
fn terrain_light_batches_match_320_native_material_combinations()
-> Result<(), Box<dyn std::error::Error>> {
    let mut cases = 0;
    for line in include_str!("../fixtures/terrain_light_batches_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let words = line.split_whitespace().collect::<Vec<_>>();
        let mut layers: [Vec<BatchLayer>; 4] = std::array::from_fn(|_| Vec::new());
        for (material, word) in layers.iter_mut().zip(&words[1..5]) {
            if *word == "-" {
                continue;
            }
            for layer in word.split(';') {
                let (texture, flags) = layer.split_once(':').ok_or("native material")?;
                material.push(BatchLayer {
                    texture: texture.parse()?,
                    flags: flags.parse()?,
                });
            }
        }
        let partners = pairing(
            words[0] == "1",
            std::array::from_fn(|i| layers[i].as_slice()),
        );
        let bounds: [[Vec3; 2]; 4] = std::array::from_fn(|i| {
            let x = (i / 2) as f32;
            let y = (i % 2) as f32;
            [
                Vec3::new(-32. * (x + 1.), -32. * (y + 1.), i as f32),
                Vec3::new(-32. * x, -32. * y, (i + 10) as f32),
            ]
        });
        for (index, partner) in partners.into_iter().enumerate() {
            let expected = &words[5 + index * 5..10 + index * 5];
            assert_eq!(index.min(partner), expected[0].parse::<usize>()?, "{line}");
            let actual = sphere([
                bounds[index][0].min(bounds[partner][0]).to_array(),
                bounds[index][1].max(bounds[partner][1]).to_array(),
            ]);
            for (component, word) in actual.to_array().into_iter().zip(&expected[1..]) {
                assert_eq!(
                    component.to_bits(),
                    word.parse::<f32>()?.to_bits(),
                    "{line}"
                );
            }
        }
        cases += 1;
    }
    assert_eq!(cases, 320);
    Ok(())
}
