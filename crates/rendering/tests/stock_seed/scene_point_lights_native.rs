use super::*;
use std::error::Error;

#[test]
fn scene_point_lights_match_original_spatial_cells_nearest_order_and_ties()
-> Result<(), Box<dyn Error>> {
    let mut bank = ScenePointLights::default();
    let mut count = 0;
    for line in include_str!("../fixtures/scene_point_light_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let row = line.split_ascii_whitespace().collect::<Vec<_>>();
        if row[0] == "lights" {
            bank.clear();
            let values = row[2..]
                .iter()
                .map(|v| u32::from_str_radix(v, 16).map(f32::from_bits))
                .collect::<Result<Vec<_>, _>>()?;
            for (index, &position) in values.as_chunks::<3>().0.iter().enumerate() {
                assert_eq!(
                    bank.publish(M2PointLight::new(
                        Vec3::from_array(position),
                        Vec3::ZERO,
                        Vec3::ONE
                    ))?,
                    index
                );
            }
        } else {
            let values = row[1..5]
                .iter()
                .map(|v| u32::from_str_radix(v, 16).map(f32::from_bits))
                .collect::<Result<Vec<_>, _>>()?;
            let result = bank.query(Vec3::from_slice(&values[..3]), values[3])?;
            let expected_count = row[5].parse::<usize>()?;
            assert_eq!(
                result.indices().into_iter().flatten().count(),
                expected_count,
                "{line}"
            );
            for slot in 0..4 {
                let expected = row[6 + slot].parse::<i32>()?;
                assert_eq!(
                    result.indices()[slot],
                    (expected >= 0).then_some(expected as usize),
                    "slot {slot}: {line}"
                );
                assert_eq!(
                    result.squared_distances()[slot].to_bits(),
                    u32::from_str_radix(row[10 + slot], 16)?,
                    "distance {slot}: {line}"
                );
            }
            count += 1;
        }
    }
    assert_eq!(count, 468);
    bank.clear();
    assert_eq!(bank.query(Vec3::ZERO, 0.)?.indices(), [None; 4]);
    Ok(())
}

#[test]
fn scene_liquid_lights_match_original_query_and_shader_upload() -> Result<(), Box<dyn Error>> {
    use crate::{LiquidFog, LiquidLighting, LiquidShaderUniform, M2DirectionalLight, WorldCamera};
    let fixture = include_bytes!("../fixtures/scene_liquid_light_native.bin");
    let records = fixture.as_chunks::<224>().0;
    assert_eq!(records.len(), 84);
    let positions = [
        [-40., 0., 4.],
        [-1., 0., 1.],
        [1., 0., 1.],
        [0., 1., 1.],
        [20., 0., 2.],
        [40., 0., 4.],
    ];
    for record in records {
        let input = record[..16]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|v| u32::from_le_bytes(*v))
            .collect::<Vec<_>>();
        let [count, directional_count, camera_index, active] = input[..] else {
            unreachable!()
        };
        let eye = [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z][camera_index as usize];
        let camera = WorldCamera::stock(
            Vec3::ZERO,
            -eye,
            if camera_index < 4 { Vec3::Z } else { Vec3::Y },
            100.,
        )
        .frame(1.)?;
        let mut bank = ScenePointLights::default();
        for (index, &position) in positions[..count as usize].iter().enumerate() {
            let index = index as f32;
            bank.publish(M2PointLight::new(
                Vec3::from_array(position),
                Vec3::ZERO,
                Vec3::new(1. + index, 64. + 16. * index, 255. - 32. * index),
            ))?;
        }
        let directional = (0..directional_count)
            .rev()
            .map(|index| {
                M2DirectionalLight::new(
                    if index == 0 { Vec3::X } else { -Vec3::Y },
                    Vec3::splat(0.0625 * (index + 1) as f32),
                    Vec3::new(0.25 * (index + 1) as f32, 0.5, 0.75),
                )
            })
            .collect::<Vec<_>>();
        let lighting = LiquidLighting::new(
            camera.view().transform_vector3(-Vec3::Z),
            Vec3::new(0.125, 0.25, 0.375),
            Vec3::new(0.5, 0.625, 0.75),
            Vec3::new(0.25, 0.375, 0.5),
        )
        .with_scene_directional_lights(camera.view(), &directional);
        let lighting = bank.liquid_lighting(Vec3::ZERO, 35., camera.view(), lighting)?;
        let uniform = LiquidShaderUniform::new(
            Mat4::IDENTITY,
            camera.view(),
            Mat4::IDENTITY,
            Mat4::IDENTITY,
            lighting,
            LiquidFog::new(Vec3::new(0., 1., 1.), Vec3::ZERO),
        )
        .to_bytes();
        for (actual, expected) in uniform[272..336]
            .as_chunks::<4>()
            .0
            .iter()
            .zip(record[16..80].as_chunks::<4>().0)
        {
            assert_eq!(
                f32::from_le_bytes(*actual),
                f32::from_le_bytes(*expected),
                "directional case {input:?}"
            );
        }
        for (actual, expected) in uniform[352..352 + active as usize * 48]
            .as_chunks::<4>()
            .0
            .iter()
            .zip(record[80..80 + active as usize * 48].as_chunks::<4>().0)
        {
            assert_eq!(
                f32::from_le_bytes(*actual),
                f32::from_le_bytes(*expected),
                "point case {input:?}"
            );
        }
    }
    Ok(())
}
