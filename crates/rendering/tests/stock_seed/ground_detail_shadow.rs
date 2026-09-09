//! Original DetailDoodad primary-shadow pixels through the production world pass.

use std::error::Error;

use glam::{Mat4, Vec3, Vec4};
use solarity_rendering::{
    GroundDetailFrame, M2LocalLightState, M2PreparedDraw, M2SceneUniform, TerrainSceneUniform,
    VulkanRenderer, WorldFrameScene, WorldModelSceneUniform, WorldPrimaryShadowFrame,
    WorldShadowProjection, WorldShadowQuality,
};

/// Retains the existing animated caster while varying baked shadows, normal relief,
/// map sizes, and receiver origin. Empty and disabled maps must produce equal pixels.
pub(super) fn compare_receivers(
    renderer: &mut VulkanRenderer,
    base: Vec3,
    casters: &[M2PreparedDraw],
    bones: &[Mat4],
) -> Result<(), Box<dyn Error>> {
    let eye = base + Vec3::Z * 20.;
    let view = Mat4::look_at_rh(eye, base, Vec3::Y);
    let projection = Mat4::orthographic_rh(-10., 10., -10., 10., 0.1, 150.);
    let fog = Vec4::new(0., 150., 0., 1.);
    let scene = WorldFrameScene::new(
        TerrainSceneUniform::new(projection, view, Vec3::ONE, Vec3::ZERO, Vec3::Z),
        WorldModelSceneUniform::new(projection, view, eye, Vec3::ONE, Vec3::ZERO, Vec3::Z, fog),
        M2SceneUniform::new(
            projection,
            view,
            eye,
            Vec3::ONE,
            Vec3::ZERO,
            Vec3::Z,
            fog,
            Vec3::ZERO,
            [M2LocalLightState::disabled(); 4],
        ),
    );
    for baked in [0, 255] {
        let draw = super::ground_detail_shader::prepare_draw_at(
            renderer,
            0xffc0_6020,
            [127; 3],
            baked,
            base + Vec3::new(16., 16., 0.),
        )?;
        let scene = scene.with_ground_detail(GroundDetailFrame::new(
            std::slice::from_ref(&draw),
            100.,
            eye,
        )?);
        for normal_dot in [0.5_f32, 1.] {
            // 7BB570 multiplies incoming Z by five before normalization. These
            // inputs yield the same dot products captured from the original PS.
            let ray = Vec3::new((1. - normal_dot * normal_dot).sqrt(), 0., -normal_dot / 5.);
            for quality in [WorldShadowQuality::UnitsLow, WorldShadowQuality::UnitsHigh] {
                for (casting, outside) in [
                    (None, false),
                    (Some(0), false),
                    (Some(casters.len()), false),
                    (Some(casters.len()), true),
                ] {
                    let center = base + if outside { Vec3::X * 100. } else { Vec3::ZERO };
                    let shadow = WorldShadowProjection::primary(quality, center, eye, ray)?;
                    let frame_scene = casting.map_or(scene, |count| {
                        scene.with_primary_shadows(WorldPrimaryShadowFrame::new(
                            shadow,
                            &casters[..count],
                        ))
                    });
                    renderer.request_frame_capture()?;
                    let report = renderer.present_world_frame(
                        frame_scene,
                        bones,
                        &[],
                        &[],
                        &[],
                        &[],
                        &[],
                        &[],
                        &[],
                        &[],
                    )?;
                    assert_eq!(report.ground_detail_draw_count(), 1);
                    let capture = renderer
                        .take_captured_frame()?
                        .ok_or("grass shadow capture")?;
                    let occupied = casting.is_some_and(|count| count > 0) && !outside;
                    let expected = native_color(baked, normal_dot, occupied)?;
                    for y in [30, 32, 34] {
                        for x in [30, 32, 34] {
                            let pixel = &capture.rgba8()[(y * 64 + x) * 4..(y * 64 + x) * 4 + 3];
                            assert!(
                                pixel.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 2),
                                "base={base:?}, baked={baked}, normal_dot={normal_dot}, quality={quality:?}, casting={casting:?}, outside={outside}: {x}/{y}={pixel:?}, native={expected:?}"
                            );
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// Reads captured bytes from unchanged DetailDoodad vertex/pixel variant one.
fn native_color(baked: u8, normal_dot: f32, occupied: bool) -> Result<[u8; 3], Box<dyn Error>> {
    let pattern = if occupied { 0 } else { 0xffff };
    for line in include_str!("../fixtures/ground-detail-shadow-native.txt")
        .lines()
        .filter(|line| line.starts_with("detail_shadow "))
    {
        let row = line.split_ascii_whitespace().collect::<Vec<_>>();
        if row[1] != "0.0"
            || row[2] != "0.0"
            || row[3] != "0.499"
            || row[4].parse::<u8>()? != baked
            || row[5].parse::<f32>()? != normal_dot
            || row[6].parse::<u32>()? != pattern
        {
            continue;
        }
        return Ok([
            u8::from_str_radix(&row[7][0..2], 16)?,
            u8::from_str_radix(&row[7][2..4], 16)?,
            u8::from_str_radix(&row[7][4..6], 16)?,
        ]);
    }
    Err("missing native grass-shadow case".into())
}
