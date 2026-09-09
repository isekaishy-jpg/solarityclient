//! Original Terrain.bls fog pixels through the uploaded ADT renderer.

use glam::{Mat4, Vec3, Vec4};
use solarity_rendering::{TerrainPreparedDraw, TerrainSceneUniform, VulkanRenderer};
use std::error::Error;

pub(super) fn compare_native_fog(
    renderer: &mut VulkanRenderer,
    draw: TerrainPreparedDraw,
) -> Result<(), Box<dyn Error>> {
    let mut frames = 0;
    for line in include_str!("../fixtures/terrain_fog_shader_native.txt")
        .lines()
        .filter(|line| line.starts_with("terrain "))
    {
        let row = line.split_ascii_whitespace().collect::<Vec<_>>();
        let depth: f32 = row[1].parse()?;
        let exponent: f32 = row[2].parse()?;
        let expected = (0..3)
            .map(|i| u8::from_str_radix(&row[3][2 * i..2 * i + 2], 16))
            .collect::<Result<Vec<_>, _>>()?;
        let eye = Vec3::new(-16., -16., depth);
        let view = Mat4::look_at_rh(eye, eye - Vec3::Z, Vec3::Y);
        for perspective in [false, true] {
            if perspective && depth == 0. {
                continue;
            }
            let projection = if perspective {
                Mat4::perspective_rh(std::f32::consts::FRAC_PI_3, 1., 0.1, 100.)
            } else {
                Mat4::orthographic_rh(-10., 10., -10., 10., -1., 100.)
            };
            let scene = TerrainSceneUniform::new(projection, view, Vec3::ONE, Vec3::ZERO, Vec3::Z)
                .with_fog(
                    view,
                    Vec4::new(0., 20., 0., exponent),
                    Vec3::new(32., 64., 96.) / 255.,
                );
            let bytes = scene.to_bytes();
            assert_eq!(f32::from_le_bytes(bytes[140..144].try_into()?), exponent);
            renderer.request_frame_capture()?;
            renderer.present_terrain(scene, &[draw])?;
            let frame = renderer
                .take_captured_frame()?
                .ok_or("missing terrain fog capture")?;
            for y in [24, 32, 40] {
                for x in [24, 32, 40] {
                    let pixel = &frame.rgba8()[(y * 64 + x) * 4..(y * 64 + x) * 4 + 3];
                    for (actual, expected) in pixel.iter().zip(&expected) {
                        assert!(
                            actual.abs_diff(*expected) <= 1,
                            "{line}, perspective={perspective}, pixel {x}/{y}: {pixel:?}"
                        );
                    }
                }
            }
            frames += 1;
        }
    }
    assert_eq!(frames, 44);
    Ok(())
}
