//! Actual glare queries, deferred visibility, native blends and slot reuse.

use super::{scene, triangle};
use glam::{Mat4, Vec3};
use solarity_rendering::{
    LiquidDepthTexture, LiquidDepthTextureKind, LiquidDrawMaterial, LiquidFog, LiquidFrame,
    LiquidLighting, LiquidRenderVertex, LiquidShaderUniform, VulkanBootstrap, WorldCamera,
    WorldCelestials, WorldGlareEnvironment, WorldGlareFrame,
};
use std::error::Error;

/// Distinguishes a whole-disc visibility factor from ordinary per-pixel depth clipping.
#[test]
#[allow(unsafe_code)] // SDL transfers the hidden surface to its owning Vulkan renderer.
fn glare_queries_fade_occluders_clouds_and_moon_across_slots() -> Result<(), Box<dyn Error>> {
    let _lock = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity glare regression", 256, 256)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: The extensions match this live SDL window; it outlives surface ownership.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (256, 256), 0) }?;
    let white = renderer.upload_stock_m2_white()?;
    let full = renderer.upload_liquid_mesh(&triangle(0.8, [0, 0, 0, 255]), &[0, 1, 2])?;
    let half_vertices = [
        [-1., -1., 0.8],
        [0., -1., 0.8],
        [0., 1., 0.8],
        [-1., 1., 0.8],
    ]
    .map(|position| {
        LiquidRenderVertex::new(position, [0., 0., 1.], [0, 0, 0, 255], [0.; 2], [0.; 2])
    });
    let half = renderer.upload_liquid_mesh(&half_vertices, &[0, 1, 2, 0, 2, 3])?;
    let uniform = LiquidShaderUniform::new(
        Mat4::IDENTITY,
        Mat4::IDENTITY,
        Mat4::IDENTITY,
        Mat4::IDENTITY,
        LiquidLighting::new(-Vec3::Z, Vec3::ONE, Vec3::ZERO, Vec3::ZERO),
        LiquidFog::new(Vec3::new(0., 1., 1.), Vec3::ZERO),
    );
    let occluders = [full, half]
        .map(|mesh| renderer.prepare_liquid_draw(mesh, LiquidDrawMaterial::Magma, white, uniform))
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    let depth = LiquidDepthTexture::prepare(LiquidDepthTextureKind::River, [0; 2], [0; 2]);
    // day, cloud alpha, water depth, sky weight, optional blocker, expected red.
    let cases = [
        (0.5, 0., None, 0., None, 32),
        (0.5, 0., None, 0., Some(1), 16),
        (0.5, 0., None, 0., Some(0), 0),
        (0.5, 0., None, 0., None, 32),
        (0.5, 1., None, 0., None, 0),
        (0.5, 0., Some(10.), 0., None, 0),
        (0.5, 0., None, 1., None, 0),
        (0., 0.5, None, 0., None, 32),
        (0., 0., None, 0., None, 0),
    ];
    for (case, (day, cloud, water, sky, blocker, expected)) in cases.into_iter().enumerate() {
        let bodies = WorldCelestials::sample(day, 20000., Vec3::ZERO).bodies();
        let selected = usize::from(day == 0.);
        let camera = WorldCamera::new(
            Vec3::ZERO,
            bodies[selected].position(),
            Vec3::Z,
            1.,
            0.5,
            777.,
        )
        .frame(1.)?;
        for step in 0..16 {
            let glare = WorldGlareFrame {
                camera,
                bodies: [bodies[0], bodies[1]],
                textures: [white; 2],
                colors: [0xff202010; 2],
                environment: WorldGlareEnvironment {
                    day,
                    elapsed_seconds: 0.1,
                    cloud_alpha: [cloud; 2],
                    liquid_depth: water,
                    skybox_weight: sky,
                },
            };
            let mut frame = scene().with_glare(glare);
            if let Some(blocker) = blocker {
                frame = frame.with_liquids(LiquidFrame::new(
                    &occluders[blocker..blocker + 1],
                    &depth,
                    &depth,
                    &depth,
                    0,
                ));
            }
            if step == 15 {
                renderer.request_frame_capture()?;
            }
            renderer.present_world_frame(frame, &[], &[], &[], &[], &[], &[], &[], &[], &[])?;
            if step == 15 {
                if matches!(case, 0 | 4 | 7) {
                    let light = renderer.world_glare_lighting().apply(Vec3::ONE);
                    let expected_light = if case == 0 { 166. } else { 255. };
                    assert!(
                        (light.x * 255. - expected_light).abs() <= 1.,
                        "case {case}: exterior light {light}"
                    );
                }
                let capture = renderer
                    .take_captured_frame()?
                    .ok_or("missing glare capture")?;
                // The half blocker covers this pixel. Native additive glare must
                // still reach it with the whole-disc visibility factor.
                let actual = capture.rgba8()[(128 * 256 + 126) * 4];
                assert!(
                    (i32::from(actual) - expected).abs() <= 2,
                    "case {case}: red {actual}, expected {expected}"
                );
                if case == 7 {
                    // At midnight the 1.75-unit disc produces 3.5-unit glare.
                    // This pixel lies beyond a fixed two-unit billboard.
                    let edge = capture.rgba8()[(128 * 256 + 102) * 4];
                    assert!(
                        (i32::from(edge) - expected).abs() <= 2,
                        "moon glare extent: {edge}"
                    );
                }
            }
        }
    }
    renderer.retire_liquid_meshes(&[full, half])?;
    Ok(())
}
