//! Pixel coverage, native face banks, depth separation, and shared-map retirement.

use std::sync::Arc;

use solarity_asset::TerrainLowDetail;
use solarity_rendering::{
    TerrainLowDetailMap, WorldCamera, WorldLowDetailFrame, WorldSkyDome, WorldSkyFrame,
};

use super::*;

/// 795F80 replaces only projection; its GX view is the ordinary camera's view.
#[test]
fn horizon_preserves_main_camera_basis_when_world_target_rounds() -> Result<(), Box<dyn Error>> {
    let map = Arc::new(flat_map([40, 29], 0)?);
    for eye in [Vec3::new(15000., -14000., 2500.), Vec3::splat(33_554_432.)] {
        for yaw in [0.1_f32, 0.7, 1.4, 3.2, 5.9] {
            let direction = Vec3::new(yaw.cos(), yaw.sin(), -0.3).normalize();
            assert_ne!(eye + direction - eye, direction);
            let camera = WorldCamera::stock(eye, eye + direction, Vec3::Z, 777.)
                .with_view_direction(direction)
                .frame(16. / 9.)?;
            let horizon = WorldLowDetailFrame::new(&map, camera, Vec3::ONE)?;
            assert_eq!(horizon.camera().view(), camera.view());
            assert_eq!(horizon.camera().forward(), camera.forward());
            assert_eq!(horizon.camera().right(), camera.right());
            assert_eq!(horizon.camera().up(), camera.up());
            assert_eq!(horizon.camera().camera().near_clip(), 727.);
            assert_eq!(horizon.camera().camera().far_clip(), 777.);
        }
    }
    Ok(())
}

#[test]
#[allow(unsafe_code)] // SDL transfers this hidden test surface to Vulkan ownership.
fn horizon_frames_preserve_native_banks_projection_and_world_depth() -> Result<(), Box<dyn Error>> {
    let _sdl_test = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity horizon test", 256, 256)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: The live window's required surface extensions were enabled.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    // SAFETY: The window outlives the renderer's sole ownership of the surface.
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (256, 256), 0) }?;
    let white = renderer.upload_stock_m2_white()?;
    // Even almost-far ordinary geometry must cover the reserved horizon interval.
    let foreground = [[0., -1., 0.9997], [4., -1., 0.9997], [0., 3., 0.9997]].map(|position| {
        LiquidRenderVertex::new(
            position,
            [0., 0., 1.],
            [255, 0, 0, 255],
            [0., 0.25],
            [0.; 2],
        )
    });
    let foreground = renderer.upload_liquid_mesh(&foreground, &[0, 1, 2])?;
    let uniform = LiquidShaderUniform::new(
        Mat4::IDENTITY,
        Mat4::IDENTITY,
        Mat4::IDENTITY,
        Mat4::IDENTITY,
        LiquidLighting::new(-Vec3::Z, Vec3::ONE, Vec3::ZERO, Vec3::ZERO),
        LiquidFog::new(Vec3::new(0., 1., 1.), Vec3::ZERO),
    );
    let draws =
        [renderer.prepare_liquid_draw(foreground, LiquidDrawMaterial::Magma, white, uniform)?];
    let depth = LiquidDepthTexture::prepare(LiquidDepthTextureKind::River, [0; 2], [0; 2]);
    let mut sky = WorldSkyDome::new();
    let mut covered = 0;
    let mut rejected = 0;
    // Map changes release the CPU owner while earlier frame slots may still use it.
    for (case, (tile_xy, mask, below, yaw)) in [
        ([32, 32], 0_u16, false, 0_f32),
        ([32, 32], 0, false, 0.08),
        ([32, 32], 0x5555, false, -0.1),
        ([32, 32], 0x5555, true, 0.05),
        ([40, 29], 0, false, 0.12),
        ([40, 29], 0xffff, false, 0.),
        ([40, 29], 0x5555, false, 0.),
        ([32, 32], 0, false, 0.),
    ]
    .into_iter()
    .enumerate()
    {
        let map = Arc::new(flat_map(tile_xy, mask)?);
        let base = Vec3::from_array(map.tiles()[0].positions()[0]);
        let sign = if below { -1. } else { 1. };
        let eye = base + Vec3::new(100., -266., sign * 100.);
        let forward = Vec3::new(-yaw.cos(), yaw.sin(), -sign * 0.3).normalize();
        let camera = WorldCamera::stock(eye, eye + forward, Vec3::Z, 350.).frame(1.)?;
        let fog = Vec3::new(50. + case as f32 * 7., 90., 140.) / 255.;
        let frame = WorldLowDetailFrame::new(&map, camera, fog)?;
        assert_eq!(frame.camera().camera().near_clip(), 300.);
        assert_eq!(frame.camera().camera().far_clip(), 350.);
        sky.update_colors(
            [Vec3::new(0.04, 0.08, 0.12); 5],
            Vec3::new(0.04, 0.08, 0.12),
            0.,
            0.,
            camera,
        );
        // A baseline records the actual sky gradient/coverage for this camera.
        let baseline = scene()
            .with_world_depth_range()
            .with_sky(WorldSkyFrame::new(&sky, camera));
        renderer.request_frame_capture()?;
        renderer.present_world_frame(baseline, &[], &[], &[], &[], &[], &[], &[], &[], &[])?;
        let background = renderer
            .take_captured_frame()?
            .ok_or("missing horizon baseline")?;
        for reuse in 0..4 {
            let mut scene = baseline.with_low_detail(frame);
            let foreground = case == 7 && reuse == 3;
            if foreground {
                scene = scene.with_liquids(LiquidFrame::new(&draws, &depth, &depth, &depth, 0));
            }
            renderer.request_frame_capture()?;
            let report =
                renderer.present_world_frame(scene, &[], &[], &[], &[], &[], &[], &[], &[], &[])?;
            assert_eq!(
                report.low_detail_draw_count(),
                if mask == 0xffff {
                    0
                } else if mask == 0 {
                    1
                } else {
                    2
                }
            );
            let capture = renderer
                .take_captured_frame()?
                .ok_or("missing horizon capture")?;
            for y in (8..248).step_by(3) {
                for x in (8..248).step_by(3) {
                    let nx = (x as f32 + 0.5) / 128. - 1.;
                    let ny = 1. - (y as f32 + 0.5) / 128.;
                    let ray = camera.forward()
                        + camera.right() * (nx / camera.projection().x_axis.x)
                        + camera.up() * (ny / camera.projection().y_axis.y);
                    let distance = (base.z - eye.z) / ray.z;
                    let point = eye + ray * distance;
                    let row = (base.x - point.x) / 33.333_332;
                    let column = (base.y - point.y) / 33.333_332;
                    // Exclude only clip/cell edges where rasterizer subpixel rules differ.
                    if (distance - 300.).abs() < 0.1
                        || (distance - 350.).abs() < 0.1
                        || (row - row.round()).abs() < 0.015
                        || (column - column.round()).abs() < 0.015
                    {
                        continue;
                    }
                    let in_tile = (0.0..16.).contains(&row) && (0.0..16.).contains(&column);
                    let marked = in_tile && mask & (1 << column as u32) != 0;
                    let visible = (300.0..350.).contains(&distance)
                        && in_tile
                        && mask != 0xffff
                        && (!marked || !below);
                    let offset = (y * 256 + x) * 4;
                    let expected = if foreground && nx > 0. {
                        Vec3::new(255., 0., 0.)
                    } else if visible {
                        covered += 1;
                        fog * 255.
                    } else {
                        rejected += 1;
                        Vec3::new(
                            f32::from(background.rgba8()[offset]),
                            f32::from(background.rgba8()[offset + 1]),
                            f32::from(background.rgba8()[offset + 2]),
                        )
                    };
                    let pixel = &capture.rgba8()[offset..offset + 3];
                    for (actual, expected) in pixel.iter().zip(expected.to_array()) {
                        assert!(
                            (f32::from(*actual) - expected).abs() <= 1.,
                            "case {case} reuse {reuse} pixel {x},{y} distance {distance} row {row} col {column} marked {marked} visible {visible}: {pixel:?}, expected {expected}"
                        );
                    }
                }
            }
        }
    }
    assert!(covered > 1000 && rejected > 1000);
    renderer.retire_liquid_meshes(&[foreground])?;
    Ok(())
}

/// Authors a single native tile so ray/plane coverage is independent of mesh generation.
fn flat_map([x, y]: [usize; 2], mask: u16) -> Result<TerrainLowDetailMap, Box<dyn Error>> {
    let mut bytes = Vec::new();
    let mut offsets = [0; 4096 * 4];
    offsets[(y * 64 + x) * 4..(y * 64 + x + 1) * 4].copy_from_slice(&16404_u32.to_le_bytes());
    for (magic, data) in [
        (b"REVM", 18_u32.to_le_bytes().to_vec()),
        (b"FOAM", offsets.to_vec()),
        (b"ERAM", vec![0; 1090]),
        (b"OHAM", mask.to_le_bytes().repeat(16)),
    ] {
        bytes.extend_from_slice(magic);
        bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&data);
    }
    let map = TerrainLowDetail::decode(&AssetPath::new("World/Maps/Flat/Flat.wdl")?, &bytes)?;
    Ok(TerrainLowDetailMap::new(&map))
}
