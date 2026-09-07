//! Liquid uniform ABI used by the independently captured D3D9 GPU frames.

use glam::{Mat4, Vec3};
use solarity_rendering::{LiquidFog, LiquidLighting, LiquidPointLight, LiquidShaderUniform};

/// Every fixture input was consumed by the original BLS shaders on Direct3D 9.
#[test]
fn liquid_uniforms_match_original_shader_frame_inputs() {
    const RECORD_SIZE: usize = 16 + 512 + 132 + 512 + 1024;
    let fixture = include_bytes!("../fixtures/liquid_shader_frames.bin");
    assert_eq!(fixture.len(), 36 * RECORD_SIZE);
    for record in fixture.as_chunks::<RECORD_SIZE>().0 {
        let points = record[4] as usize;
        let palette = record[8] as usize;
        let mut projection = Mat4::IDENTITY;
        let mut model_view = Mat4::IDENTITY;
        let mut surface = Mat4::IDENTITY;
        let mut depth = Mat4::IDENTITY;
        if palette == 3 {
            model_view = Mat4::from_scale(Vec3::new(1.25, 0.8, 1.2));
            model_view.w_axis.z = 0.1;
            projection = Mat4::from_scale(Vec3::new(0.8, 1.25, (1.0_f64 / 1.2) as f32));
            projection.w_axis.z = (-0.1_f64 / 1.2) as f32;
            surface.x_axis.x = 0.7;
            surface.x_axis.y = 0.2;
            surface.y_axis.x = -0.3;
            surface.y_axis.y = 0.6;
            surface.w_axis.x = 0.25;
            surface.w_axis.y = 0.1;
            depth.x_axis.x = 0.6;
            depth.y_axis.y = 0.8;
            depth.w_axis.x = 0.05;
            depth.w_axis.y = 0.1;
        }
        let ambient = [
            Vec3::new(0.1, 0.2, 0.3),
            Vec3::new(3.0, -0.5, 2.0),
            Vec3::new(0.3, 0.2, 0.1),
            Vec3::new(0.2, 0.15, 0.1),
        ][palette];
        let lights = std::array::from_fn::<_, 3, _>(|index| {
            LiquidPointLight::new(
                Vec3::new(1.0 + index as f32, -2.0 + index as f32, 4.0),
                Vec3::new(0.15, 0.25, 0.05),
                Vec3::new(1.0, 0.7, 0.03),
            )
        });
        let lighting = LiquidLighting::new(
            Vec3::new(0.3, 0.0, -0.953_939_2),
            ambient,
            Vec3::new(0.2, 0.3, 0.4),
            Vec3::new(0.15, 0.25, 0.35),
        )
        .with_point_lights(&lights[..points]);
        let fog = LiquidFog::new(
            if palette == 2 {
                Vec3::new(-0.5, 0.8, 1.5)
            } else {
                Vec3::new(0.0, 1.0, 1.0)
            },
            Vec3::new(8.0 / 255.0, 16.0 / 255.0, 24.0 / 255.0),
        );
        let actual =
            LiquidShaderUniform::new(projection, model_view, surface, depth, lighting, fog)
                .to_bytes();
        // Unused native light constants remain authored in the capture, while
        // the Rust draw clears their inactive positions/colors/attenuation.
        let active_end = 352 + points * 48;
        assert_eq!(&actual[..active_end], &record[16..16 + active_end]);
        assert_eq!(&actual[496..], &record[512..528]);
    }
}
